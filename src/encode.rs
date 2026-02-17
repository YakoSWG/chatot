use byteorder::{LittleEndian, WriteBytesExt};
use rayon::prelude::*;
use serde_derive::Deserialize;
use serde_json;
use std::collections::HashMap;
use std::io::Cursor;
use std::mem::size_of;

use crate::charmap;

struct MessageTableEntry {
    offset: u32,
    length: u32,
}

#[derive(Deserialize)]
struct JsonMessage {
    #[allow(dead_code)]
    id: String,
    #[serde(flatten)]
    lang_message: HashMap<String, MessageContent>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MessageContent {
    Single(String),
    Multi(Vec<String>),
}

#[derive(Deserialize)]
struct JsonInput {
    key: u16,
    messages: Vec<JsonMessage>,
}

pub fn encode_texts(
    charmap: &charmap::Charmap,
    source: &crate::TextSource,
    destination: &crate::BinarySource,
    settings: &crate::Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get list of text files
    let text_files = if let Some(files) = &source.txt {
        files.clone()
    } else if let Some(dir) = &source.text_dir {
        // Read all files from directory
        std::fs::read_dir(dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect()
    } else {
        return Err("No text source specified".into());
    };

    // Get list of archive files
    let archive_files = if let Some(files) = &destination.archive {
        files.clone()
    } else if let Some(dir) = &destination.archive_dir {
        // Create vector of archive file paths which will be created when writing
        text_files
            .iter()
            .map(|text_path| {
                let file_stem = text_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                dir.join(format!("{}", file_stem))
            })
            .collect()
    } else {
        return Err("No archive destination specified".into());
    };

    // Open and encode each text file in parallel
    let text_archive_pairs: Vec<_> = text_files
        .into_iter()
        .zip(archive_files.into_iter())
        .collect();

    let results: Vec<Result<(), String>> = text_archive_pairs
        .par_iter()
        .map(|(text_path, archive_path)| {
            // Check if newer_only setting is enabled and skip if destination is newer
            if settings.newer_only {
                if text_path.exists() && archive_path.exists() {
                    let archive_metadata = std::fs::metadata(archive_path).map_err(|e| {
                        format!(
                            "Failed to get metadata for archive {:?}: {}",
                            archive_path, e
                        )
                    })?;
                    let text_metadata = std::fs::metadata(text_path).map_err(|e| {
                        format!(
                            "Failed to get metadata for text file {:?}: {}",
                            text_path, e
                        )
                    })?;
                    let archive_modified = archive_metadata.modified().map_err(|e| {
                        format!(
                            "Failed to get modified time for archive {:?}: {}",
                            archive_path, e
                        )
                    })?;
                    let text_modified = text_metadata.modified().map_err(|e| {
                        format!(
                            "Failed to get modified time for text file {:?}: {}",
                            text_path, e
                        )
                    })?;
                    if archive_modified >= text_modified {
                        #[cfg(debug_assertions)]
                        println!(
                            "Skipping decoding of {:?} as destination {:?} is newer",
                            archive_path, text_path
                        );
                        return Ok(());
                    }
                }
            }

            #[cfg(debug_assertions)]
            println!("Encoding text: {:?} -> {:?}", text_path, archive_path);

            let text_content = std::fs::read_to_string(text_path)
                .map_err(|e| format!("Failed to read text {:?}: {}", text_path, e))?;
            let encoded_data = if settings.json {
                encode_json(&charmap, &text_content, &settings.lang)
                    .map_err(|e| format!("Failed to encode JSON {:?}: {}", text_path, e))?
            } else {
                encode_text(&charmap, &text_content, settings.msgenc_format)
                    .map_err(|e| format!("Failed to encode text {:?}: {}", text_path, e))?
            };
            std::fs::write(archive_path, encoded_data)
                .map_err(|e| format!("Failed to write archive {:?}: {}", archive_path, e))?;

            if settings.newer_only {
                // Update timestamp on source text file to match destination archive
                let archive_metadata = std::fs::metadata(archive_path).map_err(|e| {
                    format!(
                        "Failed to get metadata for archive {:?}: {}",
                        archive_path, e
                    )
                })?;
                let modified_time = archive_metadata.modified().map_err(|e| {
                    format!(
                        "Failed to get modified time for archive {:?}: {}",
                        archive_path, e
                    )
                })?;
                let text_file = std::fs::File::open(text_path)
                    .map_err(|e| format!("Failed to open text file {:?}: {}", text_path, e))?;
                text_file.set_modified(modified_time).map_err(|e| {
                    format!(
                        "Failed to update modified time for text file {:?}: {}",
                        text_path, e
                    )
                })?;
            }

            Ok(())
        })
        .collect();

    // Check for errors
    for result in results {
        result.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    }

    Ok(())
}

fn encode_text(
    charmap: &charmap::Charmap,
    text: &str,
    msgenc_format: bool,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut key = 0u16;
    let mut messages: Vec<String> = Vec::new();

    for line in text.lines() {
        // First line is key (// Key: XXXX)
        if let Some(key_str) = line.strip_prefix("// Key: ") {
            key = parse_hex_or_decimal(key_str.trim()) as u16;
            continue; // skip key line
        }

        // Ignore comment lines
        if line.trim_start().starts_with("//") {
            continue;
        }

        messages.push(line.to_string());
    }

    encode_messages(charmap, key, &messages, msgenc_format)
}

fn encode_json(
    charmap: &charmap::Charmap,
    json_content: &str,
    lang: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Some JSON files may start with a UTF-8 BOM (U+FEFF). Trim it so
    // serde_json doesn't fail with "expected value at line 1 column 1".
    let content = json_content.trim_start_matches('\u{FEFF}');
    let parsed: JsonInput = serde_json::from_str(content)?;

    let mut messages: Vec<String> = Vec::with_capacity(parsed.messages.len());

    for msg in parsed.messages.iter() {
        let content = msg
            .lang_message
            .get(lang)
            .or_else(|| msg.lang_message.get("en_US"))
            .ok_or_else(|| format!("Language '{}' not found in message {}", lang, msg.id))?;

        let message_str = match content {
            MessageContent::Single(s) => s.clone(),
            MessageContent::Multi(lines) => lines.join(""),
        };

        messages.push(message_str);
    }

    #[cfg(debug_assertions)]
    println!(
        "Encoding JSON with key: 0x{:04X}, messages: {}",
        parsed.key,
        messages.len()
    );

    encode_messages(charmap, parsed.key, &messages, false)
}

fn encode_messages(
    charmap: &charmap::Charmap,
    key: u16,
    messages: &[String],
    msgenc_format: bool,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut message_index = 0usize;

    // Create message table
    let mut message_table: Vec<MessageTableEntry> = Vec::new();

    // Collect encoded messages
    let mut encoded_messages = Vec::new();

    for message in messages {
        // Start from message index 1
        message_index += 1;

        let message_codes = encode_string_to_message(charmap, message, msgenc_format);
        let mut encrypted_codes = encrypt_message(&message_codes, message_index as u16);

        let len = encrypted_codes.len() as u32; // length in u16 units

        // If there is a previous message, calculate offset (in bytes)
        let offset = if message_table.is_empty() {
            0
        } else {
            message_table.last().unwrap().offset + (message_table.last().unwrap().length * 2)
        };

        message_table.push(MessageTableEntry {
            offset,
            length: len,
        });

        // Append encrypted message to encoded data
        encoded_messages.append(&mut encrypted_codes);
    }

    // Adjust offsets in message table to account for table itself and header
    let message_count = message_table.len();
    let table_size = message_count * size_of::<MessageTableEntry>(); // each entry
    let header_size = 4; // 2 bytes for message count + 2 bytes for key
    for entry in message_table.iter_mut() {
        entry.offset += table_size as u32 + header_size;
    }

    // Create a cursor to write binary data
    let mut cursor = Cursor::new(Vec::new());

    // Write header
    cursor.write_u16::<LittleEndian>(message_count as u16)?;
    cursor.write_u16::<LittleEndian>(key)?;

    // Write message table
    for (i, entry) in message_table.iter().enumerate() {
        // Encrypt offset and length
        let mut local_key: u32 = 765;
        local_key = local_key.wrapping_mul((i + 1) as u32);
        local_key = local_key.wrapping_mul(key as u32);
        local_key &= 0xFFFF;
        local_key |= local_key << 16;

        let enc_offset = entry.offset ^ local_key;
        let enc_length = entry.length ^ local_key;

        cursor.write_u32::<LittleEndian>(enc_offset)?;
        cursor.write_u32::<LittleEndian>(enc_length)?;
    }

    // Write encoded messages
    for code in encoded_messages.iter() {
        cursor.write_u16::<LittleEndian>(*code)?;
    }

    Ok(cursor.into_inner())
}

fn encrypt_message(decrypted_message: &Vec<u16>, index: u16) -> Vec<u16> {
    let mut encrypted_message = Vec::new();

    let mut current_key: u16 = (index as u32).wrapping_mul(596947) as u16;

    for &dec_char in decrypted_message {
        let enc_char = dec_char ^ current_key;
        encrypted_message.push(enc_char);
        current_key = current_key.wrapping_add(18749);
        current_key &= 0xFFFF;
    }

    encrypted_message
}

fn encode_string_to_message(
    charmap: &charmap::Charmap,
    text: &str,
    msgenc_format: bool,
) -> Vec<u16> {
    let mut message_codes = Vec::new();

    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        let ch_str = ch.to_string();

        // Try single character lookup
        if charmap.encode_map.contains_key(&ch_str) {
            let code = charmap.encode_map.get(&ch_str).unwrap();
            message_codes.push(*code);
            continue;
        }
        // Try multi-character aliases (wrapped in square brackets)
        else if ch == '[' {
            // Find the closing bracket
            let mut alias = String::from("[");
            let mut found_closing = false;

            while let Some(&next_ch) = chars.peek() {
                alias.push(next_ch);
                chars.next();
                if next_ch == ']' {
                    found_closing = true;
                    break;
                }
            }

            if found_closing && charmap.encode_map.contains_key(&alias) {
                let code = charmap.encode_map.get(&alias).unwrap();
                message_codes.push(*code);
                continue;
            } else if found_closing {
                eprintln!("Warning: unknown alias '{alias}'. Inserting null code.");
            } else {
                eprintln!("Warning: unmatched '[' in text. Inserting null code.");
            }
            message_codes.push(0);
            continue;
        }
        // Escape sequences (\xXXXX or \n, \r, etc.)
        else if ch == '\\' {
            if let Some(&next_ch) = chars.peek() {
                if next_ch == 'x' {
                    // Try to read hex code \xXXXX
                    chars.next(); // consume 'x'
                    let mut hex_str = String::new();
                    for _ in 0..4 {
                        if let Some(&hex_ch) = chars.peek() {
                            hex_str.push(hex_ch);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    if hex_str.len() == 4 {
                        if let Ok(code) = u16::from_str_radix(&hex_str, 16) {
                            message_codes.push(code);
                            continue;
                        } else {
                            eprintln!(
                                "Warning: invalid escape sequence '\\x{hex_str}'. Inserting null code."
                            );
                            message_codes.push(0);
                            continue;
                        }
                    } else {
                        eprintln!("Warning: incomplete hex escape sequence. Inserting null code.");
                        message_codes.push(0);
                        continue;
                    }
                } else {
                    // Try two-character escape sequence like \n, \r
                    let escape_seq = format!("\\{}", next_ch);
                    chars.next(); // consume next character

                    if charmap.encode_map.contains_key(&escape_seq) {
                        let code = charmap.encode_map.get(&escape_seq).unwrap();
                        message_codes.push(*code);
                        continue;
                    } else {
                        eprintln!(
                            "Warning: unknown escape sequence '{escape_seq}'. Inserting null code."
                        );
                        message_codes.push(0);
                        continue;
                    }
                }
            } else {
                eprintln!(
                    "Warning: incomplete escape sequence at end of text. Inserting null code."
                );
                message_codes.push(0);
                continue;
            }
        }
        // Command style sequences
        else if ch == '{' {
            // Find the closing brace
            let mut command_str = String::new();
            let mut found_closing = false;

            while let Some(&next_ch) = chars.peek() {
                if next_ch == '}' {
                    chars.next(); // consume '}'
                    found_closing = true;
                    break;
                }
                command_str.push(next_ch);
                chars.next();
            }

            if !found_closing {
                eprintln!("Warning: unmatched '{{' in text. Inserting null code.");
                message_codes.push(0);
                continue;
            }

            if command_str.is_empty() {
                eprintln!("Warning: empty command '{{}}'. Inserting null code.");
                message_codes.push(0);
                continue;
            }
            // Special handling for TRAINER_NAME command
            if command_str.starts_with("TRAINER_NAME:") {
                let name_str = &command_str["TRAINER_NAME:".len()..];
                let name_codes = encode_trainer_name(charmap, name_str);
                message_codes.extend(name_codes);
                continue;
            }
            // Handling for TRNAME command (used by msgenc)
            else if msgenc_format && command_str.starts_with("TRNAME") {
                // Treat the rest of the message as trainer name
                let name_str: String = chars.collect();
                let name_codes = encode_trainer_name(charmap, &name_str);
                message_codes.extend(name_codes);
                break; // end of message
            } else if msgenc_format {
                let command_codes = encode_command_msgenc(charmap, &command_str);
                message_codes.extend(command_codes);
                continue;
            } else {
                let command_codes = encode_command(charmap, &command_str);
                message_codes.extend(command_codes);
                continue;
            }
        }
        // Unknown character
        else {
            eprintln!("Warning: unknown character '{}'. Inserting null code.", ch);
            message_codes.push(0);
            continue;
        }
    }

    // Message termination code
    message_codes.push(0xFFFF);

    message_codes
}

fn encode_command(charmap: &charmap::Charmap, command_str: &str) -> Vec<u16> {
    let mut command_codes = Vec::new();

    // Split command and arguments
    let parts: Vec<&str> = command_str.split(',').map(|s| s.trim()).collect();

    // Ensure there is at least a command name and the special byte which is OR'ed with it
    if parts.len() < 2 {
        eprintln!(
            "Warning: invalid command format '{}'. Inserting null code.",
            command_str
        );
        command_codes.push(0);
        return command_codes;
    }

    // First part is command
    let command_name = parts[0];

    let mut command_code = match charmap
        .command_map
        .iter()
        .find(|(_, name)| *name == command_name)
    {
        Some((code, _)) => *code,
        None => {
            let code = parse_hex_or_decimal(command_name) as u16;
            eprintln!(
                "Warning: unknown command name '{}'. Using code 0x{:04X}.",
                command_name, code
            );
            code
        }
    };

    // Second part is always special byte
    let special_byte_str = parts[1];

    // Allow special byte to be in hex (0xXX) or decimal
    let special_byte = parse_hex_or_decimal(special_byte_str) as u16;

    // Push command marker
    command_codes.push(0xFFFE);

    command_code |= special_byte;
    command_codes.push(command_code);

    // Remaining parts are parameters
    let param_len = parts.len() - 2;
    command_codes.push(param_len as u16);

    for param_str in parts.iter().skip(2) {
        let param = parse_hex_or_decimal(param_str) as u16;
        command_codes.push(param);
    }
    command_codes
}

fn encode_command_msgenc(charmap: &charmap::Charmap, command_str: &str) -> Vec<u16> {
    let mut command_codes = Vec::new();

    // Opinion: I don't understand why msgenc uses this different format for commands.
    // You could just put a comma between the command name and parameters instead of using whitespace here and ONLY here.
    // Split into two parts by finding first whitespace
    let mut parts_iter = command_str.split_whitespace();
    let command_name = parts_iter.next().unwrap();

    // Split the rest by commas and remove any empty parts
    let parts: Vec<&str> = parts_iter
        .flat_map(|s| s.split(',').map(|s| s.trim()))
        .filter(|s| !s.is_empty())
        .collect();

    let mut command_code = match charmap
        .command_map
        .iter()
        .find(|(_, name)| *name == command_name)
    {
        Some((code, _)) => *code,
        None => {
            let code = parse_hex_or_decimal(command_name) as u16;
            eprintln!(
                "Warning: unknown command name '{}'. Using code 0x{:04X}.",
                command_name, code
            );
            code
        }
    };

    // Set up iterator for parameters and get parameter count
    let mut param_iter = parts.iter();
    let mut param_len = parts.len();

    // Assume this is the special byte for now
    if param_len > 0 {
        let special_byte_str = parts[0];
        let special_byte = parse_hex_or_decimal(special_byte_str);

        if command_name.starts_with("STRVAR_") {
            command_code |= special_byte as u16;
            param_iter.next(); // consume special byte
            param_len -= 1;
        }
    }

    // Push command marker
    command_codes.push(0xFFFE);
    command_codes.push(command_code);

    // Remaining parts are parameters
    command_codes.push(param_len as u16);

    let mut debug_params = Vec::new();

    for param_str in param_iter {
        let param = parse_hex_or_decimal(param_str) as u16;
        command_codes.push(param);
        debug_params.push(format!("0x{:04X}", param));
    }

    command_codes
}

fn encode_trainer_name(charmap: &charmap::Charmap, name_str: &str) -> Vec<u16> {
    let mut name_codes = Vec::new();

    name_codes.push(0xF100); // Trainer name command code

    let mut bit = 0;
    let mut current_u16 = 0u16;

    // Pack 9-bit character codes into u16s. MSB is always 0 except for terminator
    for ch in name_str.chars() {
        let code = if charmap.encode_map.contains_key(&ch.to_string()) {
            *charmap.encode_map.get(&ch.to_string()).unwrap()
        } else {
            eprintln!(
                "Warning: unknown character '{}' in trainer name. Using null code.",
                ch
            );
            0
        };

        current_u16 |= (code & 0x1FF) << bit;
        bit += 9;

        // If we've filled a u16, push it and start a new one
        if bit >= 15 {
            name_codes.push(current_u16 & 0x7FFF);
            bit -= 15;
            current_u16 = (code >> (9 - bit)) & 0x1FF;
        }
    }

    // If there are remaining bits, push the last u16
    if bit > 0 {
        // Shift the 9-bit termination code (0x1FF) into the remaining bits and emit the final u16
        current_u16 |= 0xFFFF << bit;
        name_codes.push(current_u16 & 0x7FFF);
    }

    name_codes
}

fn parse_hex_or_decimal(number_str: &str) -> u32 {
    let number = if number_str.starts_with("0x") {
        u32::from_str_radix(&number_str[2..], 16).unwrap_or(0)
    } else {
        number_str.parse::<u32>().unwrap_or(0)
    };
    number
}
