use byteorder::{LittleEndian, ReadBytesExt};
use rayon::prelude::*;
use serde_derive::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    io::Cursor,
};

use crate::charmap;

#[derive(Serialize, Deserialize, Clone)]
pub struct TextArchive {
    pub key: u16,
    pub messages: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JsonMessage {
    pub id: String,
    #[serde(flatten)]
    pub lang_message: HashMap<String, MessageContent>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Single(String),
    Multi(Vec<String>),
}

#[derive(Serialize, Deserialize, Clone)]
struct JsonOutput {
    key: u16,
    messages: Vec<JsonMessage>,
}

struct MessageTableEntry {
    offset: u32,
    length: u32,
}

pub fn decode_archives(
    charmap: &charmap::Charmap,
    source: &crate::BinarySource,
    destination: &crate::TextSource,
    settings: &crate::Settings,
) -> Result<(), Box<dyn std::error::Error>> {
    // Get list of archive files
    let archive_files = if let Some(files) = &source.archive {
        files.clone()
    } else if let Some(dir) = &source.archive_dir {
        // Read all files from directory
        std::fs::read_dir(dir)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .collect()
    } else {
        return Err("No archive source specified".into());
    };

    // Get list of text files
    let text_files = if let Some(files) = &destination.txt {
        files.clone()
    } else if let Some(dir) = &destination.text_dir {
        let extension = if settings.json { "json" } else { "txt" };

        // Create vector of text file paths which will be created when writing
        archive_files
            .iter()
            .map(|archive_path| {
                let file_stem = archive_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                dir.join(format!("{}.{}", file_stem, extension))
            })
            .collect()
    } else {
        return Err("No text destination specified".into());
    };

    // Open and decode each archive in parallel
    let archive_text_pairs: Vec<_> = archive_files
        .into_iter()
        .zip(text_files.into_iter())
        .collect();

    let results: Vec<Result<(), String>> = archive_text_pairs
        .par_iter()
        .map(|(archive_path, text_path)| {
            // Check newer_only setting is enabled and skip if destination is newer
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
                    if archive_modified <= text_modified {
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
            println!("Decoding archive: {:?} -> {:?}", archive_path, text_path);

            let archive_file = std::fs::read(archive_path)
                .map_err(|e| format!("Failed to read archive {:?}: {}", archive_path, e))?;
            let mut cursor = Cursor::new(&archive_file);
            let archive = decode_archive(&charmap, &mut cursor, settings.msgenc_format)
                .map_err(|e| format!("Failed to decode archive {:?}: {}", archive_path, e))?;

            if settings.json {
                write_decoded_json(&archive, text_path, settings.lang.clone()).map_err(|e| {
                    format!("Failed to write decoded JSON to {:?}: {}", text_path, e)
                })?;
            } else {
                write_decoded_text(&archive, text_path, settings.msgenc_format).map_err(|e| {
                    format!("Failed to write decoded text to {:?}: {}", text_path, e)
                })?;
            }

            if settings.newer_only {
                // Update source archive file timestamp to match destination text file
                let text_metadata = std::fs::metadata(text_path).map_err(|e| {
                    format!(
                        "Failed to get metadata for text file {:?}: {}",
                        text_path, e
                    )
                })?;
                let modified_time = text_metadata.modified().map_err(|e| {
                    format!(
                        "Failed to get modified time for text file {:?}: {}",
                        text_path, e
                    )
                })?;
                let archive_file = std::fs::File::open(archive_path).map_err(|e| {
                    format!("Failed to open archive file {:?}: {}", archive_path, e)
                })?;
                archive_file.set_modified(modified_time).map_err(|e| {
                    format!(
                        "Failed to update modified time for archive file {:?}: {}",
                        archive_path, e
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

fn write_decoded_text(
    archive: &TextArchive,
    text_path: &std::path::PathBuf,
    msgenc_format: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut content = archive.messages.join("\n");

    if !msgenc_format {
        // Prepend key as comment
        content = format!("// Key: 0x{:04X}\n{}", archive.key, content);
    }

    content.push('\n'); // Add trailing newline
    std::fs::write(text_path, content)?;

    Ok(())
}

fn write_decoded_json(
    archive: &TextArchive,
    text_path: &std::path::PathBuf,
    lang: String,
) -> Result<(), Box<dyn std::error::Error>> {
    // Determine archive name from text_path file name
    let archive_name = text_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("archive");

    // If JSON already exists, load it to merge languages
    let mut existing_messages: HashMap<String, JsonMessage> = HashMap::new();
    if text_path.exists() {
        if let Ok(existing_str) = std::fs::read_to_string(text_path) {
            if let Ok(existing_json) = serde_json::from_str::<JsonOutput>(&existing_str) {
                for msg in existing_json.messages {
                    existing_messages.insert(msg.id.clone(), msg);
                }
            }
        }
    }

    let mut seen_ids: HashSet<String> = HashSet::new();

    let mut json_messages: Vec<JsonMessage> = archive
        .messages
        .iter()
        .enumerate()
        .map(|(idx, msg)| {
            let id = format!("msg_{}_{:05}", archive_name, idx);
            seen_ids.insert(id.clone());

            // Split message by literal \n, \r or \f sequences
            // This gives us pretty printing while keeping the custom line breaks intact
            let mut lines: Vec<String> = Vec::new();
            let mut current = String::new();

            for ch in msg.chars() {
                current.push(ch);
                if current.ends_with("\\n") || current.ends_with("\\r") || current.ends_with("\\f")
                {
                    lines.push(current.clone());
                    current.clear();
                }
            }

            if !current.is_empty() {
                lines.push(current);
            }

            let content = if lines.len() == 1 {
                MessageContent::Single(msg.clone())
            } else {
                MessageContent::Multi(lines)
            };

            let mut merged = existing_messages.remove(&id).unwrap_or(JsonMessage {
                id: id.clone(),
                lang_message: HashMap::new(),
            });

            merged.lang_message.insert(lang.clone(), content);
            merged
        })
        .collect();

    // Preserve any existing messages not present in the current archive (GF can not be trusted)
    for (id, msg) in existing_messages.into_iter() {
        if !seen_ids.contains(&id) {
            json_messages.push(msg);
        }
    }

    let output = JsonOutput {
        key: archive.key,
        messages: json_messages,
    };

    let json_string = serde_json::to_string_pretty(&output)?;
    std::fs::write(text_path, json_string)?;

    Ok(())
}

pub fn decode_archive<R: std::io::Read + std::io::Seek>(
    charmap: &charmap::Charmap,
    reader: &mut R,
    msgenc_format: bool,
) -> Result<TextArchive, Box<dyn std::error::Error>> {
    // Read u16 message count (2 bytes)
    let message_count = reader.read_u16::<LittleEndian>()?;
    let mut messages = Vec::with_capacity((message_count as usize) * 40); // Rough estimate
    // Read u16 key (2 bytes)
    let key = reader.read_u16::<LittleEndian>()?;

    // Read message table entries
    let mut message_table = Vec::new();
    for i in 0..message_count {
        let mut offset = reader.read_u32::<LittleEndian>()?;
        let mut length = reader.read_u32::<LittleEndian>()?;

        let mut local_key: u32 = 765;
        local_key = local_key.wrapping_mul((i + 1) as u32);
        local_key = local_key.wrapping_mul(key as u32);
        local_key &= 0xFFFF;

        local_key |= local_key << 16;
        offset ^= local_key;
        length ^= local_key;

        message_table.push(MessageTableEntry { offset, length });
    }

    // Read and decode messages
    for (i, entry) in message_table.iter().enumerate() {
        // Ensure offset and length are within bounds (length is in u16 units)
        // Check if seeking to the end of the message would fail
        let end_position = entry.offset as u64 + (entry.length as u64 * 2);
        if let Err(_) = reader.seek(std::io::SeekFrom::Start(end_position)) {
            return Err(format!(
                "Invalid message entry offset/length: offset={}, length={}",
                entry.offset, entry.length
            )
            .into());
        }

        // Seek back to the actual message start position
        reader.seek(std::io::SeekFrom::Start(entry.offset as u64))?;
        let mut encrypted_message = vec![0u16; entry.length as usize];
        encrypted_message
            .iter_mut()
            .for_each(|c| *c = reader.read_u16::<LittleEndian>().unwrap());
        let decrypted_message = decrypt_message(&encrypted_message, (i + 1) as u16);

        let message_string = decode_message_to_string(&charmap, &decrypted_message, msgenc_format);
        messages.push(message_string);
    }

    Ok(TextArchive { key, messages })
}

fn decrypt_message(encrypted_message: &Vec<u16>, index: u16) -> Vec<u16> {
    let mut decrypted_message = Vec::with_capacity(encrypted_message.len());
    let mut current_key: u16 = (index as u32).wrapping_mul(596947) as u16;

    for &enc_char in encrypted_message {
        let dec_char = enc_char ^ current_key;
        decrypted_message.push(dec_char);
        current_key = current_key.wrapping_add(18749);
        current_key &= 0xFFFF;
    }

    decrypted_message
}

pub fn decode_message_to_string(
    charmap: &charmap::Charmap,
    decrypted_message: &Vec<u16>,
    msgenc_format: bool,
) -> String {
    let mut i = 0;
    let mut result = String::new();

    while i < decrypted_message.len() {
        let code = decrypted_message[i];

        // Termination character
        if code == 0xFFFF {
            break;
        // Special Command Character
        } else if code == 0xFFFE {
            let (command, to_skip) =
                decode_command(charmap, &decrypted_message[i..], msgenc_format);
            result.push_str(&command);
            i += to_skip;
        // Trainer Name
        } else if code == 0xF100 {
            let (trainer_name, to_skip) =
                decode_trainer_name(charmap, &decrypted_message[i..], msgenc_format);
            result.push_str(&trainer_name);
            i += to_skip;
        // Regular character
        } else if charmap.decode_map.contains_key(&code) {
            let character = charmap.decode_map.get(&code).unwrap();
            result.push_str(character);
            i += 1;
        }
        // Unknown character code
        else {
            eprintln!(
                "Warning: unknown character code 0x{:04X} encountered during decoding",
                code
            );
            result.push_str(&format!("\\x{:04X}", code));
            i += 1;
        }
    }

    if (i + 1) < decrypted_message.len() {
        eprintln!(
            "Warning: extra data found after termination character in message. Ignoring remaining {} character codes.",
            decrypted_message.len() - (i + 1)
        );
    }

    result
}

fn decode_command(
    charmap: &charmap::Charmap,
    message_slice: &[u16],
    msgenc_format: bool,
) -> (String, usize) {
    let mut result = String::new();
    let mut to_skip = 1; // Skip the 0xFFFE code

    // Stray command code
    if message_slice.is_empty() {
        eprintln!("Warning: stray command code 0xFFFE encountered with no following data");
        result.push_str("\\xFFFE");
        return (result, to_skip);
    }

    // Get command code
    let mut command_code = message_slice[1];
    to_skip += 1;

    // No param count (invalid)
    if message_slice.len() < 2 {
        eprintln!(
            "Warning: command code 0x{:04X} encountered with no parameter count",
            command_code
        );
        result.push_str(&format!("\\xFFFE\\x{:04X}", command_code));
        return (result, to_skip);
    }

    // Get number of parameters
    let param_count = message_slice[2];
    to_skip += 1 + param_count as usize;

    // Not enough data for parameters
    if message_slice.len() < (3 + param_count as usize) {
        eprintln!(
            "Warning: command code 0x{:04X} encountered with insufficient parameters (expected {}, found {})",
            command_code,
            param_count,
            message_slice.len() - 3
        );
        result.push_str(&format!(
            "\\xFFFE\\x{:04X}\\x{:04X}",
            command_code, param_count
        ));
        return (result, to_skip);
    }

    // Decode parameters
    let mut params = message_slice[3..(3 + param_count as usize)].to_vec();

    let mut special_byte: u16 = 0;

    if !charmap.command_map.contains_key(&command_code)
        && charmap.command_map.contains_key(&(command_code & 0xFF00))
    {
        special_byte = command_code & 0x00FF;
        command_code &= 0xFF00;
    }

    let command_str = if let Some(cmd) = charmap.command_map.get(&command_code) {
        cmd.clone()
    } else {
        eprintln!(
            "Warning: unknown command code 0x{:04X} encountered during decoding",
            command_code
        );
        format!("0x{:04X}", command_code)
    };

    // Regular format
    if !msgenc_format {
        // We always insert the special byte as the first parameter
        params.insert(0, special_byte);

        let param_str: String = params.iter().map(|p| format!("{p}, ")).collect();
        let param_str: &str = param_str.trim_end_matches(", ");

        result.push_str(&format!("{{{}, {}}}", command_str, param_str));

        (result, to_skip)
    }
    // Msgenc format
    else {
        // msgenc format omits the special byte if it is zero
        if special_byte != 0 {
            params.insert(0, special_byte);
        }

        // First parameter and command name are also only seperated by a space
        // Opinion: having whitespace delimiters AND comma delimiters is just weird and janky
        let mut param_str = String::new();
        if !params.is_empty() {
            param_str.push_str(&format!("{}", params[0]));
            for p in &params[1..] {
                param_str.push_str(&format!(", {}", p));
            }
        }

        result.push_str(&format!("{{{} {}}}", command_str, param_str));

        (result, to_skip)
    }
}

fn decode_trainer_name(
    charmap: &charmap::Charmap,
    message_slice: &[u16],
    msgenc_format: bool,
) -> (String, usize) {
    let mut result = String::new();
    let mut to_skip = 1; // Skip the 0xF100 code

    let mut bit = 0;
    let mut index = 1;
    let mut codes_consumed = 1;

    if !msgenc_format {
        result.push_str("{TRAINER_NAME:");
    } else {
        // msgenc treats the entire rest of the message as trainer name until termination where it just stops
        // this can in theory lead to issues if there are extra codes after the trainer name
        // this doesn't happen in the vanilla games but it's something to be aware of
        result.push_str("{TRNAME}");
    }

    while index < message_slice.len() {
        let mut code = (message_slice[index] >> bit) & 0x1FF;
        bit += 9;

        if bit >= 15 {
            bit -= 15;
            index += 1;
            codes_consumed += 1;

            if bit != 0 && index < message_slice.len() {
                code |= message_slice[index] << (9 - bit) & 0x1FF;
            }
        }

        // Termination character
        if code == 0x1FF {
            break;
        }

        if charmap.decode_map.contains_key(&code) {
            let character = charmap.decode_map.get(&code).unwrap();
            result.push_str(character);
        } else {
            result.push_str(&format!("0x{:04X}", code));
        }
    }

    // Close trainer name tag for non-msgenc format
    if !msgenc_format {
        result.push_str("}");
    }

    to_skip += codes_consumed;

    (result, to_skip)
}
