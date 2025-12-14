use std::collections::HashMap;
use std::path::PathBuf;

use serde_derive::Deserialize;

pub struct Charmap {
    pub encode_map: HashMap<String, u16>,
    pub decode_map: HashMap<u16, String>,
    pub command_map: HashMap<u16, String>,
}

#[derive(Deserialize)]
struct RawCharmap {
    char_map: HashMap<String, RawCharEntry>,
    command_map: HashMap<String, String>,
}

#[derive(Deserialize)]
struct RawCharEntry {
    #[serde(default)]
    char: Option<String>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
}

pub fn read_charmap(path: &PathBuf) -> Result<Charmap, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let raw: RawCharmap = serde_json::from_str(&content)?;

    let mut decode_map = HashMap::with_capacity(raw.char_map.len());
    let mut encode_map = HashMap::with_capacity(raw.char_map.len());
    let mut alias_map = HashMap::new();

    // First pass: build decode and encode maps
    for (code_str, entry) in raw.char_map {
        let code = u16::from_str_radix(&code_str, 16)
            .map_err(|e| format!("Invalid char_map key {code_str}: {e}"))?;

        if let Some(ch) = entry.char {
            if !ch.is_empty() {
                decode_map.insert(code, ch.clone());
                encode_map.entry(ch).or_insert(code);
            }
        }

        if let Some(aliases) = entry.aliases {
            for alias in aliases {
                alias_map.entry(alias.clone()).or_insert(code);
            }
        }
    }

    // Second pass: add aliases to encode map (we need to do this after the first pass to avoid conflicts)
    for (alias, code) in alias_map {
        // Basic alias validation
        if alias.is_empty() {
            eprint!("Warning: empty alias for code {code:04X} ignored\n");
            continue;
        }

        // Only insert the alias if it doesn't already exist in the encode map
        if encode_map.contains_key(&alias) {
            eprintln!("Warning: alias '{alias}' for code {code:04X} conflicts with existing entry, ignored");
            continue;
        }

        // Multi character aliases must be wrapped in square brackets
        if alias.len() > 1 && !(alias.starts_with('[') && alias.ends_with(']')) {
            eprintln!("Warning: multi-character alias '{alias}' for code {code:04X} must be wrapped in square brackets, ignored");
            continue;
        }

        encode_map.entry(alias).or_insert(code);
    }


    let mut command_map = HashMap::with_capacity(raw.command_map.len());
    for (code_str, name) in raw.command_map {
        let code = u16::from_str_radix(&code_str, 16)
            .map_err(|e| format!("Invalid command_map key {code_str}: {e}"))?;
        command_map.insert(code, name);
    }

    Ok(Charmap {
        encode_map,
        decode_map,
        command_map,
    })
}