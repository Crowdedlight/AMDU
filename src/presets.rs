use html_query_ast::parse_string;
use html_query_extractor::extract;
use iced::futures::future::ok;
use serde::Deserialize;
use std::error::Error;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;

#[derive(Debug, Clone)]
pub struct Mod {
    pub url: String,
    pub id: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ModPreset {
    pub name: String,
    pub mods: Vec<Mod>,
    raw_contents: String,
}

impl ModPreset {
    pub fn new(raw_contents: String) -> Result<Self, String> {
        // vector storing mod_ids
        let mut mods = Vec::new();

        // parse raw contents
        let name_parse = parse_string("{name: strong}").expect("parse expression failed");
        let name_output = extract(raw_contents.as_str(), &name_parse);
        let name = name_output["name"].as_str().unwrap().to_string();

        let parsed =
            parse_string("{mods: [data-type=ModContainer]| [ {name: td, url: a | @(href)} ] }")
                .expect("parse expression failed");
        let output = extract(raw_contents.as_str(), &parsed);
        let mods_list = output["mods"].as_array().unwrap();
        // loop through items
        for val in mods_list {
            // name
            let parsed_name = val["name"].as_str().unwrap();
            let parsed_url = val["url"].as_str().unwrap();

            // trim the prefix, the leftover is id
            let id = parsed_url
                .strip_prefix("https://steamcommunity.com/sharedfiles/filedetails/?id=")
                .unwrap()
                .parse::<u64>()
                .unwrap();

            // store in vector
            mods.push(Mod {
                url: parsed_url.to_string(),
                id,
                name: parsed_name.to_string(),
            });
        }

        Ok(ModPreset {
            name,
            mods,
            raw_contents,
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

impl PresetParser {
    pub async fn load_files_async(paths: Vec<PathBuf>) -> Result<Arc<Vec<ModPreset>>, String> {
        // todo go through each path, read file and parse into vec<ModPreset>
        let mut presets = Vec::new();
        for item in &paths {
            // read into string
            let contents = tokio::fs::read_to_string(item)
                .await
                .expect("File path doesn't exist");
            // create ModPreset object
            let new = ModPreset::new(contents).expect("File parsing failed");
            presets.push(new);
        }
        Ok(Arc::new(presets))
    }

    pub fn new() -> Self {
        Self {
            presets: Vec::new(),
        }
    }

    pub fn load_files(&mut self, paths: Vec<impl AsRef<Path>>) -> Result<(), String> {
        // todo go through each path, read file and parse into vec<ModPreset>
        for item in &paths {
            // read into string
            let contents = std::fs::read_to_string(item).expect("File path does not exist");
            // create ModPreset object
            let new = ModPreset::new(contents).expect("File parsing failed");
            self.presets.push(new);
        }
        Ok(())
    }

    pub fn set_modpresets(&mut self, presets: Vec<ModPreset>) -> Result<(), String> {
        self.presets = presets;
        Ok(())
    }

    pub fn get_modpresets(&self) -> Vec<ModPreset> {
        return self.presets.clone();
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
