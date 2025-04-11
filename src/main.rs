/*
 * Narukami TO - a server software reimplementation for a certain browser tank game.
 * Copyright (c) 2023-2025  Daniil Pryima
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

mod kind;

use std::collections::{HashMap, HashSet};
use std::io::stdout;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

use anyhow::Result;
use araumi_3ds::{Editor, Main, Material, MaterialTextureMap};
use crc::{Crc, CRC_32_ISO_HDLC};
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use walkdir::WalkDir;

use self::kind::ResourceDefinition;
use crate::kind::{
  ImageResource, MapResource, Resource, ResourceInfo, SoundResource, SwfLibraryResource, TextureResource,
};

fn is_path_hidden<P: AsRef<Path>>(path: P) -> bool {
  path.as_ref().components().any(|component| {
    if let Some(name) = component.as_os_str().to_str() {
      name.starts_with('.')
    } else {
      false
    }
  })
}

fn preprocess_input_files<P: AsRef<Path>>(paths: &[P]) -> Result<Vec<&Path>> {
  let mut result = Vec::new();
  for path in paths {
    let path = path.as_ref();
    if path.try_exists()? {
      result.push(path);
    }
  }
  result.sort();

  Ok(result)
}

pub static RESOURCE_DEFINITION_FILE: &str = "resource.yaml";
pub static CRC: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

#[tokio::main]
async fn main() -> Result<()> {
  let console = tracing_subscriber::fmt::layer()
    .with_writer(Arc::new(stdout()))
    .and_then(EnvFilter::from_default_env());
  tracing_subscriber::registry().with(console).init();
  info!("Hello, world!");

  let out = Path::new("out");
  let root = Path::new("resources");

  if !out.try_exists().unwrap() {
    fs::create_dir_all(out).await.unwrap();
  }

  let mtimes_file = out.join("mtimes");
  let mut resource_cached_mtimes = HashMap::new();
  let mut resource_actual_mtimes = HashMap::new();
  let mut unchanged_resources = HashSet::new();

  let mut mtime_skip_files = 0;
  let mut input_files = 0;
  let mut output_files = 0;
  let start = Instant::now();

  if mtimes_file.try_exists().unwrap() {
    info!("loading resource mtimes...");
    for entry in fs::read_to_string(&mtimes_file).await.unwrap().split('\n') {
      let entry = entry.trim();
      if let Some((file, time)) = entry.split_once(": ") {
        let time = time.parse::<u128>().unwrap();

        debug!("{}: {}", file, time);
        resource_cached_mtimes.insert(file.to_owned(), time);
      }
    }
  }

  info!("scanning resources...");
  let mut resources = Vec::new();
  for entry in WalkDir::new(root) {
    let entry = entry.unwrap();
    let path = entry.path();

    let parent = path.strip_prefix(root).unwrap();
    if is_path_hidden(parent) {
      continue;
    }

    let namespaces = get_namespaces(path).await;

    // Read full definitions
    if path.is_dir() {
      let definition_path = path.join(RESOURCE_DEFINITION_FILE);
      if !definition_path.try_exists().unwrap() {
        continue;
      }

      let definition = fs::read_to_string(&definition_path).await.unwrap();
      let mut definition: ResourceDefinition = serde_yaml::from_str(&definition)
        .unwrap_or_else(|error| panic!("failed to read definition {}: {error}", definition_path.display()));
      definition.resource_mut().init_root(path.to_path_buf());

      let name = path
        .strip_prefix(root)?
        .components()
        .map(|component| component.as_os_str().to_str().unwrap())
        .filter(|component| !component.starts_with("@"))
        .collect::<Vec<_>>()
        .join(".");
      let mut id = CRC.checksum(path.to_string_lossy().to_string().as_bytes());
      if let ResourceDefinition::Object3D(resource) = &definition {
        if let Some(forced_id) = resource.id {
          id = forced_id;
        }
      }
      debug!(?name, ?id, ?namespaces, "resource");

      let mut raw_input_files = definition.resource().input_files().await?;
      raw_input_files.push(definition_path.clone());
      let preprocessed_input_files = preprocess_input_files(&raw_input_files)?;

      let mtime_input_files = preprocessed_input_files.clone();

      let mut changed = false;
      for file in &mtime_input_files {
        if file.is_dir() {
          continue;
        }

        let cache_path = file.strip_prefix(root).unwrap().to_str().unwrap();

        let actual_mtime = fs::metadata(file)
          .await
          .unwrap()
          .modified()
          .map(|time| time.duration_since(UNIX_EPOCH).unwrap().as_millis())
          .expect("unsupported platform");
        resource_actual_mtimes.insert(cache_path.to_owned(), actual_mtime);

        if let Some(cached_mtime) = resource_cached_mtimes.get(cache_path) {
          if actual_mtime == *cached_mtime {
            debug!("{} has not changed", file.display());
            continue;
          }

          debug!("{} has changed", file.display());
          changed = true;
        } else {
          debug!("new file {}", file.display());
          changed = true;
        }
      }

      if name.contains("localization") {
        warn!("regenerate localization {}", name);
      } else {
        if !changed {
          debug!("skipping {} as no files have been changed", name);
          mtime_skip_files += 1;
          unchanged_resources.insert(id as i64);
          // continue;
        }
      }

      let mut digest = CRC.digest();
      for file in &preprocessed_input_files {
        if file.is_dir() {
          continue;
        }

        trace!("using {} to calculate version for {}", file.display(), id);
        digest.update(&fs::read(file).await.unwrap());
        input_files += 1;
      }
      let version = digest.finalize();

      definition
        .resource_mut()
        .init(ResourceInfo {
          name: name.clone(),
          id: id as i64,
          version: version as i64,
          namespaces: namespaces.clone(),
        })
        .await?;
      debug!(
        "read resource definition {}: {:?}",
        definition_path.display(),
        definition
      );

      resources.push(definition);
    }

    // Read short definitions
    if path.is_file() {
      let file_name = path.file_name().unwrap().to_str().unwrap();
      let (file_name, extension) = file_name.rsplit_once('.').unwrap_or((&file_name, ""));
      if let Some((name, kind)) = file_name.rsplit_once('@') {
        debug!(?name, ?kind, ?extension, "discovered short resource");

        let mut definition = match kind {
          "Sound" => ResourceDefinition::Sound(SoundResource {
            root: Default::default(),
            info: None,
            sound: Some(path.to_path_buf()),
          }),
          "Map" => ResourceDefinition::Map(MapResource {
            root: Default::default(),
            info: None,
            map: Some(path.to_path_buf()),
            parsed: None,
            proplibs: Default::default(),
            namespace: None,
          }),
          "Proplib" => unimplemented!("use full resource definition"),
          "Texture" => ResourceDefinition::Texture(TextureResource {
            root: Default::default(),
            info: None,
            diffuse: Some(path.to_path_buf()),
          }),
          "Image" => ResourceDefinition::Image(ImageResource {
            root: Default::default(),
            info: None,
            image: Some(path.to_path_buf()),
          }),
          "MultiframeTexture" => unimplemented!("use full resource definition"),
          "LocalizedImage" => unimplemented!("use full resource definition"),
          "Object3D" => unimplemented!("use full resource definition"),
          "SwfLibrary" => ResourceDefinition::SwfLibrary(SwfLibraryResource {
            root: Default::default(),
            info: None,
            file: Some(path.to_path_buf()),
          }),
          _ => unimplemented!("{} is not implemented", kind),
        };
        definition
          .resource_mut()
          .init_root(path.parent().unwrap().to_path_buf());

        let name = path
          .strip_prefix(root)?
          .parent()
          .unwrap()
          .components()
          .map(|component| component.as_os_str().to_str().unwrap())
          .filter(|component| !component.starts_with("@"))
          .collect::<Vec<_>>()
          .join(".")
          + "."
          + name;
        let id = CRC.checksum(path.to_string_lossy().to_string().as_bytes());
        debug!(?name, ?id, ?namespaces, "resource");

        let mut raw_input_files = definition.resource().input_files().await?;
        raw_input_files.push(path.to_owned());
        let preprocessed_input_files = preprocess_input_files(&raw_input_files)?;

        let mtime_input_files = preprocessed_input_files.clone();

        let mut changed = false;
        for file in &mtime_input_files {
          if file.is_dir() {
            continue;
          }

          let cache_path = file.strip_prefix(root).unwrap().to_str().unwrap();

          let actual_mtime = fs::metadata(file)
            .await
            .unwrap()
            .modified()
            .map(|time| time.duration_since(UNIX_EPOCH).unwrap().as_millis())
            .expect("unsupported platform");
          resource_actual_mtimes.insert(cache_path.to_owned(), actual_mtime);

          if let Some(cached_mtime) = resource_cached_mtimes.get(cache_path) {
            if actual_mtime == *cached_mtime {
              debug!("{} has not changed", file.display());
              continue;
            }

            debug!("{} has changed", file.display());
            changed = true;
          } else {
            debug!("new file {}", file.display());
            changed = true;
          }
        }

        if !changed {
          debug!("skipping {} as no files have been changed", name);
          mtime_skip_files += 1;
          unchanged_resources.insert(id as i64);
          // continue;
        }

        let mut digest = CRC.digest();
        for file in &preprocessed_input_files {
          if file.is_dir() {
            continue;
          }

          trace!("using {} to calculate version for {}", file.display(), name);
          digest.update(&fs::read(file).await.unwrap());
          input_files += 1;
        }
        let version = digest.finalize();

        definition
          .resource_mut()
          .init(ResourceInfo {
            name: name.clone(),
            id: id as i64,
            version: version as i64,
            namespaces: namespaces.clone(),
          })
          .await?;
        debug!("read short resource definition {}: {:?}", path.display(), definition);

        resources.push(definition);
      }
    }
  }

  let mut proplibs = resources
    .iter()
    .filter(|resource| matches!(resource, ResourceDefinition::Proplib(_)))
    .cloned()
    .collect::<Vec<_>>();

  info!("validating proplibs...");
  for definition in &mut proplibs {
    if let ResourceDefinition::Proplib(resource) = definition {
      let root = resource.get_root();

      for entry in WalkDir::new(&resource.get_root()) {
        let entry = entry?;
        if entry.file_type().is_dir() {
          continue;
        }
        if entry.file_name() == "library.xml" {
          debug!("found library.xml for {}", resource.get_info().as_ref().unwrap().name);
          let content = fs::read_to_string(entry.path()).await.unwrap();
          let deserializer = &mut quick_xml::de::Deserializer::from_str(&content);
          resource.library = Some(serde_path_to_error::deserialize(deserializer)?);
        }
        if entry.file_name() == "images.xml" {
          debug!("found images.xml for {}", resource.get_info().as_ref().unwrap().name);
          let content = fs::read_to_string(entry.path()).await.unwrap();
          let deserializer = &mut quick_xml::de::Deserializer::from_str(&content);
          resource.images = Some(serde_path_to_error::deserialize(deserializer)?);
        }
      }

      if let Some(images) = &resource.images {
        for image in &images.images {
          trace!("{:?}", image);

          let file = root.join(&image.diffuse);
          let file = file_exists_case_insensitive(&file);
          if let Some(_file) = &file {
          } else {
            error!("proplib: {:?}", resource.get_info());
            panic!("diffuse file {:?} for texture {} not exists", file, image.name);
          }

          if let Some(alpha) = &image.alpha {
            let file = root.join(alpha);
            let file = file_exists_case_insensitive(&file);
            if let Some(_file) = &file {
            } else {
              panic!("alpha file {:?} for texture {} not exists", file, image.name);
            }
          }
        }
      }

      // let library = resource.library.as_ref().unwrap();
      // for group in &library.prop_groups {
      //   for prop in &group.props {
      //     if let Some(mesh) = &prop.mesh {
      //       let mesh_file = root.join(&mesh.file);
      //       let mesh_file = file_exists_case_insensitive(&mesh_file);
      //       if let Some(mesh_file) = &mesh_file {
      //         let data = fs::read(mesh_file).await.unwrap();
      //         let mut data = Cursor::new(data.as_slice());
      //         let mut parser = araumi_3ds::Parser3DS::new(&mut data);
      //         let main = &parser.read_main()[0];
      //         let default_texture = get_texture_map_name(&main);
      //         if let Some(default_texture) = &default_texture {
      //           let default_file = file_exists_case_insensitive(root.join(default_texture));
      //           if let Some(default_file) = &default_file {
      //             // info!("{:?}", default_file);
      //           } else {
      //             warn!("mesh {}/{}/{} ({:?}) default texture {} not exists", library.name, group.name, prop.name, mesh_file, default_texture);
      //           }
      //         } else {
      //           panic!("mesh {}/{}/{} ({:?}) has no default texture map", library.name, group.name, prop.name, mesh_file);
      //         }
      //       } else {
      //         panic!("mesh {}/{}/{} file {:?} not exists", library.name, group.name, prop.name, mesh_file);
      //       }

      //       // for texture in &mesh.textures {
      //       //   info!("texture {:?}", texture);
      //       // }
      //     }
      //   }
      // }
      // info!("{:?}", library);
      // info!("{:?}", images);
    } else {
      unreachable!();
    }
  }
  // return Ok(());

  info!("discovered {} resources", resources.len());

  {
    debug!("writing mtimes file...");
    let mut mtimes_file = File::create(mtimes_file).await.unwrap();
    for (file, mtime) in resource_actual_mtimes {
      mtimes_file
        .write_all(format!("{}: {}\n", file, mtime).as_bytes())
        .await
        .unwrap();
    }
    mtimes_file.flush().await.unwrap();
  }

  let mut processed_resources = 0;
  for definition in &mut resources {
    let info = definition.resource().get_info().as_ref().unwrap();
    if unchanged_resources.contains(&info.id) {
      continue;
    }

    if let ResourceDefinition::Map(resource) = definition {
      debug!("initializing map {:?}", resource.get_info().as_ref().unwrap());
      resource.init_proplibs(&proplibs).await?;
      resource.validate_props(&proplibs).await?;
    }

    let info = definition.resource().get_info().as_ref().unwrap();
    let path = out.join(info.encode());
    // .join(info.id.to_string())
    // .join(info.version.to_string());
    if path.try_exists()? {
      warn!(
        "skipping {:?} ({}) as directory already exists, cache is probably corrupt",
        info,
        path.display()
      );
      // continue;
    }

    fs::create_dir_all(&path).await?;
    processed_resources += 1;

    info!("writing output files for {:?}", info);
    debug!("writing output files for {:?}", definition);
    for (name, data) in &definition.resource().output_files().await? {
      fs::write(path.join(name), data).await?;
      debug!("written {}:{}/{}", info.id, info.version, name);

      output_files += 1;
    }
  }

  fs::write("out/00-resources.json", serde_json::to_vec_pretty(&resources)?).await?;

  let end = Instant::now();
  info!("completed in {:?}", end - start);
  info!(
    "processed {} resources ({} cached, {} not changed): generated {} files from {} files",
    processed_resources,
    resources.len() - processed_resources,
    mtime_skip_files,
    output_files,
    input_files
  );

  Ok(())
}

fn file_exists_case_insensitive<P: AsRef<Path>>(filename: P) -> Option<PathBuf> {
  let filename_str = filename.as_ref().file_name().unwrap().to_str().unwrap().to_lowercase();
  let parent_dir = filename.as_ref().parent().unwrap_or_else(|| Path::new("."));

  for entry in WalkDir::new(parent_dir).max_depth(1).into_iter().flatten() {
    if entry.file_type().is_file() {
      let entry_filename = entry.file_name().to_str().unwrap().to_lowercase();
      if entry_filename == filename_str {
        return Some(entry.into_path());
      }
    }
  }

  None
}

#[allow(irrefutable_let_patterns)]
fn get_texture_map_name(main: &Main) -> Option<String> {
  if let Main::Editor(editors) = main {
    for editor in editors {
      if let Editor::Material(materials) = editor {
        for material in materials {
          if let Material::TextureMap(texture_maps) = material {
            for texture_map in texture_maps {
              if let MaterialTextureMap::Name(name) = texture_map {
                return Some(name.clone());
              }
            }
          }
        }
      }
    }
  }
  None
}

async fn get_namespaces(path: &Path) -> HashMap<String, String> {
  let mut namespaces = HashMap::new();

  for component in path.components() {
    if let Some(comp_str) = component.as_os_str().to_str() {
      // Check if the component matches the pattern @key=value
      if comp_str.starts_with('@') {
        let parts: Vec<&str> = comp_str[1..].split('=').collect();
        if parts.len() == 2 {
          let key = parts[0].to_string();
          let value = parts[1].to_string();
          namespaces.insert(key, value);
        }
      }
    }
  }

  namespaces
}
