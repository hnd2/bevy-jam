// Example code that deserializes and serializes the model.
// extern crate serde;
// #[macro_use]
// extern crate serde_derive;
// extern crate serde_json;
//
// use generated_module::[object Object];
//
// fn main() {
//     let json = r#"{"answer": 42}"#;
//     let model: [object Object] = serde_json::from_str(&json).unwrap();
// }

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct AsepriteData {
    pub frames: HashMap<String, FrameValue>,
    pub meta: Meta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameValue {
    pub frame: SpriteSourceSizeClass,
    pub rotated: bool,
    pub trimmed: bool,
    #[serde(rename = "spriteSourceSize")]
    pub sprite_source_size: SpriteSourceSizeClass,
    #[serde(rename = "sourceSize")]
    pub source_size: Size,
    pub duration: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpriteSourceSizeClass {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Size {
    pub w: i64,
    pub h: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub app: String,
    pub version: String,
    pub image: String,
    pub format: String,
    pub size: Size,
    pub scale: String,
    #[serde(rename = "frameTags")]
    pub frame_tags: Vec<FrameTag>,
    pub layers: Vec<Layer>,
    pub slices: Vec<Option<serde_json::Value>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FrameTag {
    pub name: String,
    pub from: i64,
    pub to: i64,
    pub direction: String,
    pub color: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub opacity: i64,
    #[serde(rename = "blendMode")]
    pub blend_mode: String,
}

