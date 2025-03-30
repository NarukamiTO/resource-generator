mod image;
mod localization;
mod localized_image;
mod map;
mod multiframe_texture;
mod object3d;
mod proplib;
mod sound;
mod swf_library;
mod texture;

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize, Serializer};

pub use self::image::*;
pub use self::localization::*;
pub use self::localized_image::*;
pub use self::map::*;
pub use self::multiframe_texture::*;
pub use self::object3d::*;
pub use self::proplib::*;
pub use self::sound::*;
pub use self::swf_library::*;
pub use self::texture::*;

#[derive(Clone, Debug, Serialize)]
pub struct ResourceInfo {
  pub name: String,
  pub id: i64,
  pub version: i64,
  pub namespaces: HashMap<String, String>,
}

impl ResourceInfo {
  pub fn encode(&self) -> String {
    format!(
      "{:o}/{:o}/{:o}/{:o}/{:o}",
      (self.id >> 32) & 0xffffffff,
      (self.id >> 16) & 0xffff,
      (self.id >> 8) & 0xff,
      self.id & 0xff,
      self.version
    )
  }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResourceDefinition {
  SwfLibrary(SwfLibraryResource),
  Sound(SoundResource),
  Map(MapResource),
  Proplib(ProplibResource),
  Texture(TextureResource),
  Image(ImageResource),
  MultiframeTexture(MultiframeTextureResource),
  // ScalableImage, // Missing in old client
  LocalizedImage(LocalizedImageResource),
  Object3D(Object3DResource),
  // Effects (unused)
  // RawData (unused)
  Localization(LocalizationResource),
}

impl ResourceDefinition {
  pub fn resource(&self) -> &dyn Resource {
    match self {
      ResourceDefinition::SwfLibrary(resource) => resource,
      ResourceDefinition::Sound(resource) => resource,
      ResourceDefinition::Map(resource) => resource,
      ResourceDefinition::Proplib(resource) => resource,
      ResourceDefinition::Texture(resource) => resource,
      ResourceDefinition::Image(resource) => resource,
      ResourceDefinition::MultiframeTexture(resource) => resource,
      ResourceDefinition::LocalizedImage(resource) => resource,
      ResourceDefinition::Object3D(resource) => resource,
      ResourceDefinition::Localization(resource) => resource,
    }
  }

  pub fn resource_mut(&mut self) -> &mut dyn Resource {
    match self {
      ResourceDefinition::SwfLibrary(resource) => resource,
      ResourceDefinition::Sound(resource) => resource,
      ResourceDefinition::Map(resource) => resource,
      ResourceDefinition::Proplib(resource) => resource,
      ResourceDefinition::Texture(resource) => resource,
      ResourceDefinition::Image(resource) => resource,
      ResourceDefinition::MultiframeTexture(resource) => resource,
      ResourceDefinition::LocalizedImage(resource) => resource,
      ResourceDefinition::Object3D(resource) => resource,
      ResourceDefinition::Localization(resource) => resource,
    }
  }
}

#[async_trait]
pub trait Resource {
  fn init_root(&mut self, root: PathBuf);
  async fn init(&mut self, info: ResourceInfo) -> Result<()>;

  fn get_root(&self) -> PathBuf;
  fn get_info(&self) -> &Option<ResourceInfo>;

  async fn input_files(&self) -> Result<Vec<PathBuf>>;
  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>>;
}

#[derive(Debug, Clone)]
#[rustfmt::skip]
pub enum ResourceKind {
  SwfLibrary, // Unused
  Sound,
  Map,
  Proplib,
  Texture,
  Image,
  MultiframeTexture,
  ScalableImage,
  LocalizedImage,
  Object3D,
  Effects, // Unused
  RawData, // Unused
}

impl From<&ResourceKind> for i32 {
  fn from(kind: &ResourceKind) -> Self {
    use ResourceKind::*;

    match kind {
      SwfLibrary => 1,
      Sound => 4,
      Map => 7,
      Proplib => 8,
      Texture => 9,
      Image => 10,
      MultiframeTexture => 11,
      ScalableImage => 12,
      LocalizedImage => 13,
      Object3D => 17,
      Effects => 25,
      RawData => 400,
    }
  }
}

impl Serialize for ResourceKind {
  fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_i32(i32::from(self))
  }
}
