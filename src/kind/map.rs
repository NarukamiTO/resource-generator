use std::{
  collections::{HashMap, HashSet},
  fmt::{Debug, Formatter},
  path::PathBuf
};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, warn};

use super::Resource;
use crate::kind::{ResourceDefinition, ResourceInfo};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename = "map")]
pub struct MapXml {
  #[serde(rename = "static-geometry")]
  pub static_geometry: StaticGeometry,
  #[serde(rename = "collision-geometry")]
  pub collision_geometry: CollisionGeometry,
  #[serde(default, rename = "spawn-points")]
  pub spawn_points: SpawnPoints
}

impl MapXml {
  fn as_public(&self) -> PublicMap {
    PublicMap {
      static_geometry: &self.static_geometry,
      collision_geometry: &self.collision_geometry
    }
  }

  fn as_private(&self, proplibs: &HashMap<String, ResourceDefinition>) -> PrivateMap {
    PrivateMap {
      spawn_points: self
        .spawn_points
        .spawn_points
        .iter()
        .map(|point| point.as_private())
        .collect(),
      proplibs: proplibs
        .iter()
        .map(|(_, definition)| definition.resource().get_info().as_ref().unwrap().clone())
        .collect()
    }
  }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "map")]
pub struct PublicMap<'a> {
  #[serde(rename = "static-geometry")]
  pub static_geometry: &'a StaticGeometry,
  #[serde(rename = "collision-geometry")]
  pub collision_geometry: &'a CollisionGeometry
}

#[derive(Clone, Debug, Serialize)]
pub struct PrivateMap<'a> {
  #[serde(rename = "spawn-points")]
  pub spawn_points: Vec<PrivateSpawnPoint<'a>>,
  pub proplibs: Vec<ResourceInfo>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticGeometry {
  #[serde(rename = "prop")]
  pub props: Vec<Prop>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Prop {
  #[serde(rename = "@library-name")]
  pub library_name: String,
  #[serde(rename = "@group-name")]
  pub group_name: String,
  #[serde(rename = "@name")]
  pub name: String,
  pub position: Vector3,
  pub rotation: Vector3,
  #[serde(default, rename = "texture-name")]
  pub texture_name: String
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionGeometry {
  #[serde(rename = "collision-plane")]
  pub planes: Vec<CollisionPlane>,
  #[serde(rename = "collision-box")]
  pub boxes: Vec<CollisionBox>,
  #[serde(rename = "collision-triangle")]
  pub triangles: Vec<CollisionTriangle>
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionPlane {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub width: f32,
  pub length: f32,
  pub position: Vector3,
  pub rotation: Vector3
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionBox {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub size: Vector3,
  pub position: Vector3,
  pub rotation: Vector3
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionTriangle {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub v0: Vector3,
  pub v1: Vector3,
  pub v2: Vector3,
  pub position: Vector3,
  pub rotation: Vector3
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SpawnPoints {
  #[serde(rename = "spawn-point")]
  pub spawn_points: Vec<SpawnPoint>
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SpawnPoint {
  #[serde(rename = "@type")]
  pub kind: String,
  pub position: Vector3,
  pub rotation: Vector3
}

impl SpawnPoint {
  fn as_private(&self) -> PrivateSpawnPoint {
    PrivateSpawnPoint {
      kind: &self.kind,
      position: self.position.clone(),
      rotation: self.rotation.clone()
    }
  }
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct PrivateSpawnPoint<'a> {
  pub kind: &'a str,
  pub position: Vector3,
  pub rotation: Vector3
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Vector3 {
  #[serde(default)]
  pub x: f32,
  #[serde(default)]
  pub y: f32,
  #[serde(default)]
  pub z: f32
}

#[derive(Debug, Serialize)]
#[serde(rename = "proplibs")]
pub struct ProplibsXml {
  #[serde(rename = "library")]
  pub libraries: Vec<LibraryXml>
}

#[derive(Debug, Serialize)]
pub struct LibraryXml {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "@resource-id")]
  pub id: String,
  #[serde(rename = "@version")]
  pub version: String
}

#[derive(Clone, Serialize, Deserialize)]
pub struct MapResource {
  #[serde(skip_deserializing)]
  pub root: PathBuf,
  #[serde(skip_deserializing)]
  pub info: Option<ResourceInfo>,
  #[serde(skip)]
  pub parsed: Option<MapXml>,
  #[serde(skip)]
  pub proplibs: HashMap<String, ResourceDefinition>,

  pub map: Option<PathBuf>,
  pub namespace: Option<String>
}

impl Debug for MapResource {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct(stringify!(MapResource))
      .field("root", &self.root)
      .field("info", &self.info)
      .field(
        "parsed",
        &if self.parsed.is_some() {
          "MapXml { ... }"
        } else {
          "None"
        }
      )
      .field("proplibs", &self.proplibs)
      .field("map", &self.map)
      .field("namespace", &self.namespace)
      .finish()
  }
}

#[async_trait]
impl Resource for MapResource {
  fn init_root(&mut self, root: PathBuf) {
    self.root = root;
  }

  async fn init(&mut self, info: ResourceInfo) -> Result<()> {
    self.info = Some(info);
    Ok(())
  }

  fn get_root(&self) -> PathBuf {
    self.root.clone()
  }

  fn get_info(&self) -> &Option<ResourceInfo> {
    &self.info
  }

  async fn input_files(&self) -> Result<Vec<PathBuf>> {
    Ok(vec![self.get_map()])
  }

  async fn output_files(&self) -> Result<HashMap<String, Vec<u8>>> {
    let proplibs = ProplibsXml {
      libraries: self
        .proplibs
        .iter()
        .map(|(name, definition)| {
          let info = definition.resource().get_info().as_ref().unwrap();
          LibraryXml {
            name: name.clone(),
            id: format!("{:x}", info.id),
            version: format!("{:x}", info.version)
          }
        })
        .collect()
    };

    let parsed = self.parsed.as_ref().unwrap();
    Ok(HashMap::from([
      (
        "map.xml".to_owned(),
        quick_xml::se::to_string(&parsed.as_public())?.into_bytes()
      ),
      (
        "proplibs.xml".to_owned(),
        quick_xml::se::to_string(&proplibs)?.into_bytes()
      ),
      (
        "private.json".to_owned(),
        serde_json::to_vec_pretty(&parsed.as_private(&self.proplibs))?
      )
    ]))
  }
}

impl MapResource {
  pub fn get_map(&self) -> PathBuf {
    self
      .map
      .clone()
      .map(|file| {
        if file.starts_with(&self.root) {
          file
        } else {
          self.get_root().join(file)
        }
      })
      .unwrap_or_else(|| self.get_root().join("map.xml"))
  }

  pub async fn init_proplibs(&mut self, resources: &[ResourceDefinition]) -> Result<()> {
    let map = self.get_map();
    let map = fs::read_to_string(map).await.unwrap();
    let map: MapXml = quick_xml::de::from_str(&map)?;

    let proplib_names: HashSet<_> = map
      .static_geometry
      .props
      .iter()
      .map(|prop| &prop.library_name)
      .collect();
    for definition in resources {
      if let ResourceDefinition::Proplib(resource) = definition {
        let name = resource.name.as_ref().unwrap();
        if proplib_names.contains(name) && self.namespace == resource.namespace {
          debug!("resolved proplib {}", name);
          self.proplibs.insert(name.clone(), definition.clone());
        }
      }
    }

    for name in proplib_names {
      if !self.proplibs.contains_key(name) {
        warn!(
          "proplib {} not found for namespace {:?}",
          name, self.namespace
        );
      }
    }
    self.parsed = Some(map);

    Ok(())
  }
}
