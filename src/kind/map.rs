use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::{Debug, Formatter};
use std::io::Cursor;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use proplib::Texture;
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, error, info, warn};

use super::{proplib, ProplibResource, Resource};
use crate::kind::{ResourceDefinition, ResourceInfo};
use crate::{file_exists_case_insensitive, get_texture_map_name};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename = "map")]
pub struct MapXml {
  #[serde(rename = "static-geometry")]
  pub static_geometry: StaticGeometry,
  #[serde(rename = "collision-geometry")]
  pub collision_geometry: CollisionGeometry,
  #[serde(default, rename = "spawn-points")]
  pub spawn_points: SpawnPoints,
}

impl MapXml {
  fn as_public(&self) -> PublicMap {
    PublicMap {
      static_geometry: &self.static_geometry,
      collision_geometry: &self.collision_geometry,
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
        .collect(),
    }
  }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename = "map")]
pub struct PublicMap<'a> {
  #[serde(rename = "static-geometry")]
  pub static_geometry: &'a StaticGeometry,
  #[serde(rename = "collision-geometry")]
  pub collision_geometry: &'a CollisionGeometry,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrivateMap<'a> {
  #[serde(rename = "spawn-points")]
  pub spawn_points: Vec<PrivateSpawnPoint<'a>>,
  pub proplibs: Vec<ResourceInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StaticGeometry {
  #[serde(rename = "prop")]
  pub props: Vec<Prop>,
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
  #[serde(rename = "texture-name")]
  pub texture_name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionGeometry {
  #[serde(rename = "collision-plane")]
  pub planes: Vec<CollisionPlane>,
  #[serde(rename = "collision-box")]
  pub boxes: Vec<CollisionBox>,
  #[serde(rename = "collision-triangle")]
  pub triangles: Vec<CollisionTriangle>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionPlane {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub width: f32,
  pub length: f32,
  pub position: Vector3,
  pub rotation: Vector3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionBox {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub size: Vector3,
  pub position: Vector3,
  pub rotation: Vector3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollisionTriangle {
  #[serde(default, rename = "@id", skip_serializing_if = "Option::is_none")]
  pub id: Option<i32>,
  pub v0: Vector3,
  pub v1: Vector3,
  pub v2: Vector3,
  pub position: Vector3,
  pub rotation: Vector3,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SpawnPoints {
  #[serde(rename = "spawn-point")]
  pub spawn_points: Vec<SpawnPoint>,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct SpawnPoint {
  #[serde(rename = "@type")]
  pub kind: String,
  pub position: Vector3,
  pub rotation: Vector3,
}

impl SpawnPoint {
  fn as_private(&self) -> PrivateSpawnPoint {
    PrivateSpawnPoint {
      kind: &self.kind,
      position: self.position.clone(),
      rotation: self.rotation.clone(),
    }
  }
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct PrivateSpawnPoint<'a> {
  pub kind: &'a str,
  pub position: Vector3,
  pub rotation: Vector3,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct Vector3 {
  #[serde(default)]
  pub x: f32,
  #[serde(default)]
  pub y: f32,
  #[serde(default)]
  pub z: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename = "proplibs")]
pub struct ProplibsXml {
  #[serde(rename = "library")]
  pub libraries: Vec<LibraryXml>,
}

#[derive(Debug, Serialize)]
pub struct LibraryXml {
  #[serde(rename = "@name")]
  pub name: String,
  #[serde(rename = "@resource-id")]
  pub id: String,
  #[serde(rename = "@version")]
  pub version: String,
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
  pub namespace: Option<String>,
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
        },
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
            version: format!("{:x}", info.version),
          }
        })
        .collect(),
    };

    let parsed = self.parsed.as_ref().unwrap();
    info!("static geometry: {} props", parsed.static_geometry.props.len());
    info!("collision geometry: {} boxes, {} planes, {} triangles", parsed.collision_geometry.boxes.len(), parsed.collision_geometry.planes.len(), parsed.collision_geometry.triangles.len());
    Ok(HashMap::from([
      (
        "map.xml".to_owned(),
        quick_xml::se::to_string(&parsed.as_public())?.into_bytes(),
      ),
      (
        "proplibs.xml".to_owned(),
        quick_xml::se::to_string(&proplibs)?.into_bytes(),
      ),
      (
        "private.json".to_owned(),
        serde_json::to_vec_pretty(&parsed.as_private(&self.proplibs))?,
      ),
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
        if proplib_names.contains(name) {
          debug!("resolved proplib {}", name);
          self.proplibs.insert(name.clone(), definition.clone());
        }
      }
    }

    for name in proplib_names {
      if !self.proplibs.contains_key(name) {
        warn!("proplib {} not found for namespace {:?}", name, self.namespace);
      }
    }
    self.parsed = Some(map);

    Ok(())
  }

  pub async fn validate_props(&mut self, resources: &[ResourceDefinition]) -> Result<()> {
    info!("validating props for {:?}", self.get_info());

    let mut versions: HashMap<BTreeMap<String, String>, Vec<&ProplibResource>> = HashMap::new();
    for definition in resources {
      if let ResourceDefinition::Proplib(resource) = definition {
        let namespaces = &resource.get_info().as_ref().unwrap().namespaces;
        versions.entry(namespaces.iter().map(|(k, v)| (k.to_owned(), v.to_owned())).collect())
          .or_insert_with(Vec::new)
          .push(resource);
      }
    }

    for (namespaces, resources) in &versions {
      info!("checking proplibs {:?}: {:?} proplibs", namespaces, resources.len());
      if !namespaces.contains_key("gen") || !namespaces.contains_key("theme") {
        warn!("malformed proplibs combination: {:?}", namespaces);
        continue;
      }

      // build index
      let mut props =
        HashMap::<(String, String, String), (&ProplibResource, &proplib::PropGroup, proplib::Prop)>::default();
      for resource in resources {
        let library = resource.library.as_ref().unwrap();
        for group in &library.prop_groups {
          for prop in &group.props {
            props.insert(
              (library.name.to_owned(), group.name.to_owned(), prop.name.to_owned()),
              (resource, group, prop.clone()),
            );
          }
        }
      }

      let map = self.parsed.as_ref().unwrap();
      'prop: for map_prop in &map.static_geometry.props {
        if let Some((proplib, group, prop)) = props.get(&(
          map_prop.library_name.clone(),
          map_prop.group_name.clone(),
          map_prop.name.clone(),
        )) {
          // info!("found prop {:?} in {:?}", map_prop, prop);
          let root = proplib.get_root();
          let library = proplib.library.as_ref().unwrap();
          if let Some(mesh) = &prop.mesh {
            let mesh_file = root.join(&mesh.file);
            let mesh_file = file_exists_case_insensitive(&mesh_file);

            // info!("texture-name: {:?}, prop: {:?}", map_prop.texture_name, prop.name);
            let (texture_name, texture) = if !map_prop.texture_name.is_empty() {
              (
                map_prop.texture_name.to_owned(),
                mesh
                  .textures
                  .iter()
                  .find(|texture| texture.name == map_prop.texture_name)
                  .cloned(),
              )
            } else {
              if let Some(mesh_file) = &mesh_file {
                let data = fs::read(mesh_file).await.unwrap();
                let mut data = Cursor::new(data.as_slice());
                let mut parser = araumi_3ds::Parser3DS::new(&mut data);
                let main = &parser.read_main()[0];
                let default_texture = get_texture_map_name(main);
                if let Some(default_texture) = &default_texture {
                  (
                    default_texture.to_owned(),
                    Some(Texture {
                      name: default_texture.to_owned(),
                      diffuse_map: default_texture.to_owned(),
                    }),
                  )

                  // let default_file = file_exists_case_insensitive(root.join(default_texture));
                  // if let Some(default_file) = &default_file {
                  //   // info!("{:?}", default_file);
                  //   (default_texture.to_owned(), Some(Texture {
                  //     name: default_texture.to_owned(),
                  //     diffuse_map: default_texture.to_string_lossy().into_owned()
                  //   }))
                  // } else {
                  //   (default_texture.to_owned(), None)
                  //   // panic!("mesh {}/{}/{} ({:?}) default texture {} not exists", library.name, group.name, prop.name, mesh_file, default_texture);
                  // }
                } else {
                  panic!(
                    "mesh {}/{}/{} ({:?}) has no default texture map",
                    library.name, group.name, prop.name, mesh_file
                  );
                }
              } else {
                panic!(
                  "mesh {}/{}/{} file {:?} not exists",
                  library.name, group.name, prop.name, mesh_file
                );
              }
            };
            // info!("texture {}: {:?}", texture_name, texture);

            if let Some(texture) = &texture {
              if let Some(images) = &proplib.images {
                let image = images
                  .images
                  .iter()
                  .find(|image| image.name.to_lowercase() == texture.diffuse_map.to_lowercase());
                // info!("texture_file: {:?}", image);
                if let Some(image) = image {
                  // info!("{:?}", image);

                  let file = root.join(&image.diffuse);
                  let file = file_exists_case_insensitive(&file);
                  if let Some(_file) = &file {} else {
                    panic!("diffuse file {:?} for texture {} not exists", file, image.name);
                  }

                  if let Some(alpha) = &image.alpha {
                    let file = root.join(alpha);
                    let file = file_exists_case_insensitive(&file);
                    if let Some(_file) = &file {} else {
                      panic!("alpha file {:?} for texture {} not exists", file, image.name);
                    }
                  }
                } else {
                  error!("images: {:?}", images);
                  panic!(
                    "texture mapping for {:?} not exists for prop {}/{}/{}",
                    texture, library.name, group.name, prop.name
                  );
                }
                continue 'prop;
              } else {
                // info!("texture_file: {:?}", texture.diffuse_map);
                let file = root.join(&texture.diffuse_map);
                let file = file_exists_case_insensitive(&file);
                if let Some(_file) = &file {} else {
                  error!("prop: {:?}", map_prop);
                  error!("texture: {:?}", texture);
                  panic!("diffuse file {:?} for texture {} not exists", file, texture_name);
                }
                continue 'prop;
              }
            } else {
              panic!(
                "texture {} not exists for prop {}/{}/{}",
                texture_name, library.name, group.name, prop.name
              );
            }

            // let default_file = file_exists_case_insensitive(root.join(default_texture));
            // if let Some(default_file) = &default_file {
            //   // info!("{:?}", default_file);
            //   default_file.to_owned()
            // } else {
            //   panic!("mesh {}/{}/{} ({:?}) default texture {} not exists", library.name, group.name, prop.name, mesh_file, default_texture);
            // }

            // for texture in &mesh.textures {
            //   info!("texture {:?}", texture);
            // }
          } else if let Some(sprite) = &prop.sprite {
            if let Some(images) = &proplib.images {
              let image = images
                .images
                .iter()
                .find(|image| image.name.to_lowercase() == sprite.file.to_lowercase());
              // info!("texture_file: {:?}", image);
              if let Some(image) = image {
                // info!("{:?}", image);

                let file = root.join(&image.diffuse);
                let file = file_exists_case_insensitive(&file);
                if let Some(_file) = &file {} else {
                  panic!("diffuse file {:?} for sprite {} not exists", file, image.name);
                }

                if let Some(alpha) = &image.alpha {
                  let file = root.join(alpha);
                  let file = file_exists_case_insensitive(&file);
                  if let Some(_file) = &file {} else {
                    panic!("alpha file {:?} for sprite {} not exists", file, image.name);
                  }
                }
              } else {
                error!("images: {:?}", images);
                panic!(
                  "texture mapping for sprite {:?} not exists for prop {}/{}/{}",
                  sprite, library.name, group.name, prop.name
                );
              }
              continue 'prop;
            } else {
              let file = root.join(&sprite.file);
              let file = file_exists_case_insensitive(&file);
              if let Some(_file) = &file {
                continue 'prop;
              } else {
                panic!(
                  "sprite {}/{}/{} file {:?} not exists",
                  library.name, group.name, prop.name, sprite.file
                );
              }
            }
          } else {
            unreachable!();
          }
        } else {
          panic!("prop {:?} not found", map_prop);
        }
      }
    }

    Ok(())
  }
}
