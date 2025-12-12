use std::collections::HashMap;

use serde_derive::Deserialize;
use quick_xml::{Reader, de::{Deserializer, from_str}};


#[derive(Debug, Deserialize)]
#[serde(rename = "charmap")]
pub struct CharmapXML {
    #[serde(rename = "header")]
    pub _header: Header,
    #[serde(rename = "entry")]
    pub entries: Vec<Entry>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    #[serde(rename = "description")]
    pub _description: String,
    #[serde(rename = "version")]
    pub _version: String,
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    /// The hex code for the character/command (e.g., "0000", "FF00")
    #[serde(rename = "@code", )]
    pub code: String,
    /// The type of entry (e.g., "char", "command", "alias")
    #[serde(rename = "@kind", )]
    pub kind: String,
    /// The inner text content of the entry
    #[serde(rename = "$value", )]
    pub content: Option<String>,
}

pub struct Charmap {
    pub _encode_map: HashMap<String, u16>,
    pub decode_map: HashMap<u16, String>,
    pub command_map: HashMap<u16, String>,
}

pub fn read_charmap(path: &std::path::PathBuf) -> Result<Charmap, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;

    let mut reader = Reader::from_str(&content);
    reader.config_mut().trim_text(false);
    
    let mut deserializer = Deserializer::from_reader(reader.into_inner());
    let charmap: CharmapXML = serde::Deserialize::deserialize(&mut deserializer)?;

    let mut encode_map = HashMap::new();
    let mut decode_map = HashMap::new();
    let mut command_map = HashMap::new();

    for entry in charmap.entries {
        let code = u16::from_str_radix(&entry.code, 16)?;
        match entry.kind.as_str() {
            "char" => {
                if let Some(ch) = entry.content {
                    encode_map.insert(ch.clone(), code);
                    decode_map.insert(code, ch);                    
                }
            }
            "command" => {
                if let Some(cmd) = entry.content {
                    command_map.insert(code, cmd);                    
                }
            }
            "alias" => {
                if let Some(alias) = entry.content {
                    encode_map.insert(alias.clone(), code);                    
                }
            }
            _ => {
                eprint!("Unknown entry kind: {}", entry.kind);
            }
        }
    }    

    Ok(Charmap {
        _encode_map: encode_map,
        decode_map,
        command_map,
    })
}