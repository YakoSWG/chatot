use std::{io::Cursor};
use byteorder::{ReadBytesExt, LittleEndian};

use crate::charmap;

struct MessageTableEntry {
    offset: u32,
    length: u32,
}

pub fn decode_archives(
    charmap: &charmap::Charmap,
    source: &crate::BinarySource,
    destination: &crate::TextSource,
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
        // Create vector of text file paths which will be created when writing
        archive_files
            .iter()
            .map(|archive_path| {
                let file_stem = archive_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output");
                dir.join(format!("{}.txt", file_stem))
            })
            .collect()
    } else {
        return Err("No text destination specified".into());
    };

    println!("Archive files: {:?}", archive_files);
    println!("Text files: {:?}", text_files);

    // Open and decode each archive
    for (archive_path, text_path) in archive_files.iter().zip(text_files.iter()) {
        println!("Decoding archive: {:?}", archive_path);
        let archive_file = std::fs::read(archive_path)?;
        let lines = decode_archive(&charmap, &archive_file)?;
        std::fs::write(text_path, lines.join("\n"))?;
        println!("Decoded text written to: {:?}", text_path);
    }

    Ok(())
}

pub fn decode_archive(charmap: &charmap::Charmap, archive_file: &Vec<u8>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    
    let mut archive = Cursor::new(archive_file  );

    // Read u16 message count (2 bytes)
    let message_count = archive.read_u16::<LittleEndian>()?;

    let mut lines = Vec::with_capacity(message_count as usize);

    // Read u16 key (2 bytes)
    let key = archive.read_u16::<LittleEndian>()?;

    // Read message table entries
    let mut message_table = Vec::new();
    for i in 0..message_count {
        let mut offset = archive.read_u32::<LittleEndian>()?;
        let mut length = archive.read_u32::<LittleEndian>()?;

        let mut local_key: u32 = ((765 * (i+1) * key) & 0xFFFF).into();
        local_key |= local_key << 16;
        offset ^= local_key;
        length ^= local_key;

        message_table.push(MessageTableEntry { offset, length });
    }

    // Read and decode messages
    for (i, entry) in message_table.iter().enumerate() {
        
        // Ensure offset and length are within bounds (length is in u16 units)
        if (entry.offset as usize + (entry.length * 2) as usize) > archive.get_ref().len() {
            return Err("Invalid message entry offset/length".into());
        }

        archive.set_position(entry.offset as u64);
        let mut encrypted_message = vec![0u16; entry.length as usize];
        encrypted_message
            .iter_mut()
            .for_each(|c| *c = archive.read_u16::<LittleEndian>().unwrap());
        let decrypted_message = decrypt_message(&encrypted_message, i as u16);

        let message_string = decode_message_to_string(&charmap, &decrypted_message);
        lines.push(message_string);
    }

    Ok(lines)
}


pub fn decrypt_message(encrypted_message: &Vec<u16>, index: u16) -> Vec<u16> {
    let mut decrypted_message = Vec::with_capacity(encrypted_message.len());
    let mut current_key: u16 = (index as u32 * 596947u32) as u16;

    for &enc_char in encrypted_message {
        let dec_char = enc_char ^ current_key;
        decrypted_message.push(dec_char);
        current_key = (current_key + 18749) & 0xFFFF;
    }

    decrypted_message
}

pub fn decode_message_to_string(charmap: &charmap::Charmap, decrypted_message: &Vec<u16>) -> String {

    let mut i = 0;
    let mut result = String::new();

    while i < decrypted_message.len() {

        let code = decrypted_message[i];

        // Termination character
        if code == 0xFFFF {
            break;
        }
        // Special Command Character
        else if code == 0xFFFE {
            let (command, to_skip) = decode_command(charmap, &decrypted_message[i..]);
            result.push_str(&command);
            i += to_skip;
        }
        // Trainer Name
        else if code == 0xF100 {
            let (trainer_name, to_skip) = decode_trainer_name(charmap, &decrypted_message[i..]);
            result.push_str(&trainer_name);
            i += to_skip;
        }
        // Regular character
        else if charmap.encode_map.contains_key(&code.to_string()) {
            let character = charmap.decode_map.get(&code).unwrap();
            result.push_str(character);
            i += 1;
        }
        // Unknown character code
        else {
            result.push_str(&format!("0x{:04X}", code));
            i += 1;
        }

    }

    result
    
}

pub fn decode_command(charmap: &charmap::Charmap, message_slice: &[u16]) -> (String, usize) {
    let mut result = String::new();
    let mut to_skip = 1; // Skip the 0xFFFE code

    // Stray command code
    if message_slice.len() < 1 {
            result.push_str("\\xFFFE");
        return (result, to_skip);
    }

    // Get command code
    let mut command_code = message_slice[1];
    to_skip += 1;

    // No param count (invalid)
    if message_slice.len() < 2 {
        result.push_str(&format!("\\xFFFE\\x{:04X}", command_code));
        return (result, to_skip);
    }

    // Get number of parameters
    let param_count = message_slice[2];
    to_skip += 1 + param_count as usize;

    // Not enough data for parameters
    if message_slice.len() < (3 + param_count as usize) {
        result.push_str(&format!("\\xFFFE\\x{:04X}\\x{:04X}", command_code, param_count));
        return (result, to_skip);
    }

    // Decode parameters
    let mut params = message_slice[3..(3 + param_count as usize)].to_vec();

    let mut special_byte: u16 = 0;

    if !charmap.command_map.contains_key(&command_code) && charmap.command_map.contains_key(&(command_code & 0xFF00)) {
        special_byte = command_code & 0x00FF;
        command_code = command_code & 0xFF00;     
    }

    let command_str = if let Some(cmd) = charmap.command_map.get(&command_code) {
        cmd.clone()
    } else {
        format!("0x{:04X}", command_code)
    };

    params.insert(0, special_byte);

    let param_str: String = params.iter().map(|p| format!("{p}, ")).collect();
    let param_str: &str = param_str.trim_end_matches(", ");

    result.push_str(&format!("{{{}, {}}}", command_str, param_str));

    (result, to_skip)
}

pub fn decode_trainer_name(charmap: &charmap::Charmap, message_slice: &[u16]) -> (String, usize) {
    let mut result = String::new();
    let mut to_skip = 1; // Skip the 0xF100 code

    let mut bit = 0;
    let mut index = 1;
    let mut codes_consumed = 1;

    result.push_str("{TRAINER_NAME:");

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

    result.push_str("}");
    to_skip += codes_consumed;

    (result, to_skip)
}