#![deny(clippy::all)]

use html_query_ast::parse_string;
use html_query_extractor::extract;
use regex::Regex;
use std::cmp::Ordering;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Mod {
    pub tags: Vec<String>,
    pub url: String,
    pub id: u64,
    pub name: String,
    pub local_filesize: u64,
}
impl PartialEq for Mod {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Mod {}

impl Ord for Mod {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

impl PartialOrd for Mod {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct ModPreset {
    pub name: String,
    pub mods: Vec<Mod>,
}

impl ModPreset {
    pub fn new(raw_contents: String, filename: Option<&OsStr>) -> Result<Self, String> {
        // vector storing mod_ids
        let mut mods = Vec::new();

        // parse raw contents
        let name_parse = parse_string("{name: strong}").expect("parse expression failed");
        let name_output = extract(raw_contents.as_str(), &name_parse);

        // if name doens't exist, we use filename?
        let name = match name_output["name"].as_str() {
            Some(str) => str,
            None => filename.unwrap().to_str().unwrap(),
        };

        let parsed =
            parse_string("{mods: [data-type=ModContainer]| [ {name: td, url: a | @(href)} ] }")
                .expect("parse expression failed");
        let output = extract(raw_contents.as_str(), &parsed);
        let mods_list = output["mods"].as_array().unwrap();

        //regex build
        let re = Regex::new(r"(?<id>\d{4,})").unwrap();

        // loop through items
        for val in mods_list {
            // name
            let parsed_name = val["name"].as_str().unwrap();
            let parsed_url = val["url"].as_str().unwrap();

            // regex to get id and parse it
            let caps = re.find(parsed_url);

            if caps.is_none() {
                println!("current mod: '{}', has no url with id in it", parsed_url);
                continue;
            }

            let id = caps.unwrap().as_str().parse::<u64>().unwrap();

            // store in vector
            mods.push(Mod {
                tags: vec![],
                url: parsed_url.to_string(),
                id,
                name: parsed_name.to_string(),
                local_filesize: 0,
            });
        }

        Ok(ModPreset {
            name: name.to_string(),
            mods,
        })
    }

    pub fn get_id_list(&self) -> Vec<u64> {
        return self.mods.iter().map(|f| f.id).collect();
    }
}

#[derive(Debug, Clone)]
pub struct PresetParser {
    presets: Vec<ModPreset>,
}

impl Default for PresetParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetParser {
    pub async fn load_files_async(paths: Vec<PathBuf>) -> Result<Arc<Vec<ModPreset>>, String> {
        let mut presets = Vec::new();
        for item in &paths {
            // read into string
            let contents = tokio::fs::read_to_string(item)
                .await
                .expect("File path doesn't exist");
            // create ModPreset object
            let new = ModPreset::new(contents, item.file_name()).expect("File parsing failed");
            presets.push(new);
        }
        Ok(Arc::new(presets))
    }

    pub fn new() -> Self {
        Self {
            presets: Vec::new(),
        }
    }

    pub fn set_modpresets(&mut self, presets: Vec<ModPreset>) -> Result<(), String> {
        self.presets = presets;
        Ok(())
    }

    pub fn get_modpresets(&self) -> Vec<ModPreset> {
        self.presets.clone()
    }

    pub fn get_all_mod_ids_unique(&self) -> Result<Vec<u64>, String> {
        // make vec object
        let mut all_mods = Vec::new();

        // loop all presets
        for set in &self.presets {
            all_mods.append(&mut set.get_id_list());
        }

        // only keep unique entries
        all_mods.sort_unstable();
        all_mods.dedup();

        Ok(all_mods)
    }
}
