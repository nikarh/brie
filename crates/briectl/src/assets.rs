use std::{
    collections::{HashMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
};

use brie_cfg::Brie;
use brie_download::{download_file, MP, USER_AGENT_HEADER};
use image::GenericImageView;
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};
use log::{debug, error, info, warn};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP error. {0}")]
    Http(#[from] Box<ureq::Error>),
    #[error("IO error. {0}")]
    Io(#[from] std::io::Error),
    #[error("URL error. {0}")]
    Url(#[from] url::ParseError),
    #[error("Invalid URL.")]
    InvalidUrl,
    #[error("Image error. {0}")]
    Image(#[from] image::ImageError),
    #[error("PNG error. {0}")]
    Png(#[from] png::EncodingError),
    #[error("JSON error. {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Deserialize)]
struct Container<T> {
    data: T,
}

#[derive(Deserialize)]
struct AutocompleteResponse {
    id: u32,
}

fn autocomplete(token: &str, name: &str) -> Result<Option<u32>, Error> {
    info!("Finding steamgriddb id for `{name}`");

    let mut url = url::Url::parse("https://www.steamgriddb.com/api/v2/search/autocomplete")?;
    url.path_segments_mut()
        .map_err(|()| Error::InvalidUrl)?
        .push(name);

    let res: Container<Vec<AutocompleteResponse>> = ureq::request_url("GET", &url)
        .set("User-Agent", USER_AGENT_HEADER)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(Box::new)?
        .into_json()?;

    Ok(res.data.first().map(|r| r.id))
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub enum ImageKind {
    Grid,
    Icon,
    Hero,
}

impl ImageKind {
    fn all() -> [ImageKind; 3] {
        [ImageKind::Grid, ImageKind::Icon, ImageKind::Hero]
    }

    fn path(self) -> &'static str {
        match self {
            ImageKind::Grid => "grids",
            ImageKind::Icon => "icons",
            ImageKind::Hero => "heroes",
        }
    }
}

impl std::fmt::Display for ImageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ImageKind::Grid => "grid",
            ImageKind::Icon => "icon",
            ImageKind::Hero => "hero",
        })
    }
}

impl ImageKind {
    fn filter(self, images: &[ImageResponse]) -> Option<&str> {
        match self {
            ImageKind::Grid => images
                .iter()
                .find(|img| img.width == 600)
                .or(images.first())
                .map(|img| img.url.as_str()),
            ImageKind::Icon => images.first().map(|img| img.thumb.as_str()),
            ImageKind::Hero => images.first().map(|img| img.thumb.as_str()),
        }
    }
}

#[derive(Deserialize)]
struct ImageResponse {
    url: String,
    thumb: String,
    width: u32,
}

fn image(token: &str, kind: ImageKind, id: u32) -> Result<Option<Vec<u8>>, Error> {
    info!("Downloading and re-encoding `{kind}` image for {id}");

    let url = format!(
        "https://www.steamgriddb.com/api/v2/{kind}/game/{id}",
        kind = kind.path()
    );

    let res: Container<Vec<ImageResponse>> = ureq::get(&url)
        .set("User-Agent", USER_AGENT_HEADER)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(Box::new)?
        .into_json()?;

    let Some(url) = kind.filter(&res.data) else {
        return Ok(None);
    };

    let (mut lib, pb) = download_file(url)?.progress(format!("{id}-{kind}"));

    let mut img = Vec::new();
    lib.read_to_end(&mut img)?;
    pb.finish();

    let pb = MP.add(
        ProgressBar::new_spinner()
            .with_message(format!("Converting {id}-{kind} to png"))
            .with_finish(ProgressFinish::AndLeave)
            .with_style(
                ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg:>15}")
                    .unwrap(),
            ),
    );

    let png = convert_to_png(&img)?;

    pb.finish_with_message(format!("Converted {id}-{kind} to png"));

    Ok(Some(png))
}

fn convert_to_png(image: &[u8]) -> Result<Vec<u8>, Error> {
    let image = image::load_from_memory(image)?;
    let (width, height) = image.dimensions();

    let (color_type, image) = match &image {
        image::DynamicImage::ImageLuma8(_) | image::DynamicImage::ImageLuma16(_) => {
            (png::ColorType::Grayscale, image.into_luma8().into_vec())
        }
        image::DynamicImage::ImageLumaA8(_) | image::DynamicImage::ImageLumaA16(_) => (
            png::ColorType::GrayscaleAlpha,
            image.into_luma_alpha8().into_vec(),
        ),
        image::DynamicImage::ImageRgb8(_)
        | image::DynamicImage::ImageRgb16(_)
        | image::DynamicImage::ImageRgb32F(_) => {
            (png::ColorType::Rgb, image.into_rgb8().into_vec())
        }
        _ => (png::ColorType::Rgba, image.into_rgba8().into_vec()),
    };

    let mut png: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(color_type);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Default);
    encoder.set_filter(png::FilterType::NoFilter);

    let mut writer = encoder.write_header()?;
    writer.write_image_data(&image)?;
    drop(writer);

    Ok(png)
}

fn ensure_steamgriddb_ids(
    token: &str,
    cache_dir: &Path,
    config: &Brie,
) -> Result<HashMap<String, u32>, Error> {
    let steamgriddb_cache = cache_dir.join("steamgriddb_ids.json");

    let mut cached_ids: HashMap<String, Option<u32>> = std::fs::read(&steamgriddb_cache)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    info!("Finding missing steamgriddb ids");

    // Find ids in steamgriddb for units missing it. Ideally it should append it to `brie.yaml`, but
    // that might be complicated, considering formatting and comments should remain intact.
    let found_ids = config
        .units
        .par_iter()
        .map(|(k, v)| (k, v.common()))
        .filter(|(k, v)| !cached_ids.contains_key(*k) && v.steamgriddb_id.is_none())
        .filter_map(
            |(k, v)| match autocomplete(token, v.name.as_ref().unwrap_or(k)) {
                Ok(Some(id)) => Some((k, Some(id))),
                Ok(None) => {
                    warn!("No id found for unit {k} in steamgriddb");
                    Some((k, None))
                }
                Err(e) => {
                    error!("Failed to find id for {k}: {e}");
                    None
                }
            },
        )
        .map(|(k, v)| (k.to_owned(), v))
        .collect::<HashMap<_, _>>();

    if !found_ids.is_empty() {
        debug!("Found ids: {found_ids:?}");

        cached_ids.extend(found_ids);
        let cached_ids = serde_json::to_vec(&cached_ids)?;
        std::fs::write(&steamgriddb_cache, cached_ids)?;
    }

    let cached_ids = cached_ids
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)));

    let mut predefined_ids = config
        .units
        .iter()
        .map(|(k, v)| (k, v.common()))
        .filter_map(|(k, v)| v.steamgriddb_id.map(|id| (k.to_owned(), id)))
        .collect::<HashMap<_, _>>();

    predefined_ids.extend(cached_ids);

    Ok(predefined_ids)
}

fn ensure_images_exist(
    token: &str,
    cache_dir: &Path,
    id_map: HashMap<String, u32>,
) -> HashMap<String, Images> {
    let _ = std::fs::create_dir_all(cache_dir.join("images"));

    let ids: HashSet<_> = id_map.values().copied().collect();
    let paths: HashMap<_, _> = ids
        .par_iter()
        .cloned()
        .flat_map(|id| ImageKind::all().map(|kind| (id, kind)))
        .into_par_iter()
        .filter_map(|(id, kind)| {
            let path = cache_dir.join("images").join(format!("{id}-{kind}.png"));
            if path.exists() {
                return Some(((id, kind), path));
            }

            match image(token, kind, id) {
                Ok(Some(img)) => {
                    std::fs::write(&path, img).unwrap();
                    Some(((id, kind), path))
                }
                Ok(None) => {
                    warn!("No image found for id {id}");
                    None
                }
                Err(e) => {
                    error!("Failed to download image for id {id}: {e}");
                    None
                }
            }
        })
        .collect();

    id_map
        .into_iter()
        .map(|(key, id)| {
            (
                key,
                Images(
                    ImageKind::all()
                        .iter()
                        .filter_map(|&kind| paths.get(&(id, kind)).map(|p| (kind, p.clone())))
                        .collect(),
                ),
            )
        })
        .collect()
}

#[derive(Default, Clone)]
pub struct Images(HashMap<ImageKind, PathBuf>);

impl Images {
    pub fn get(&self, kind: ImageKind) -> Option<&PathBuf> {
        self.0.get(&kind)
    }
}

pub fn download_all(cache_dir: &Path, config: &Brie) -> Result<HashMap<String, Images>, Error> {
    let Some(token) = config.steamgriddb_token.as_ref() else {
        warn!("steamgriddb_token is not defined in the config");
        return Ok(HashMap::default());
    };

    info!("Downloading banners and icons from steamgriddb");
    let _ = std::fs::create_dir_all(cache_dir);
    let id_map = ensure_steamgriddb_ids(token, cache_dir, config)?;
    Ok(ensure_images_exist(token, cache_dir, id_map))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use brie_download::MP;
    use indicatif_log_bridge::LogWrapper;

    use crate::assets::ImageKind;

    use super::{autocomplete, download_all, image};

    const TOKEN: &str = "82e919fd236407ddbf5012fdb1b13126";

    #[test]
    pub fn test_autocomplete() {
        let res = autocomplete(TOKEN, "The witcher 3").unwrap();
        assert_eq!(res, Some(12833));
    }

    #[test]
    pub fn test_banners() {
        let res = image(TOKEN, ImageKind::Grid, 4265).unwrap().unwrap();
        assert!(res == std::fs::read("tests/grid.png").unwrap());
        let res = image(TOKEN, ImageKind::Icon, 4265).unwrap().unwrap();
        assert!(res == std::fs::read("tests/icon.png").unwrap());
    }

    #[test]
    pub fn test_download_all() {
        let log = simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Info)
            .env();
        LogWrapper::new(MP.clone(), log).try_init().unwrap();

        let cache_dir = Path::new(".tmp/cache");
        let config = brie_cfg::Brie {
            steamgriddb_token: Some(TOKEN.to_owned()),
            units: [
                (
                    "witcher3".to_owned(),
                    brie_cfg::Unit::Native(brie_cfg::NativeUnit {
                        common: brie_cfg::UnitCommon {
                            name: Some("The Witcher 3".to_owned()),
                            ..Default::default()
                        },
                    }),
                ),
                (
                    "outerwilds".to_owned(),
                    brie_cfg::Unit::Wine(brie_cfg::WineUnit {
                        common: brie_cfg::UnitCommon {
                            name: Some("The Witcher 3".to_owned()),
                            ..Default::default()
                        },
                        ..brie_cfg::WineUnit::default()
                    }),
                ),
            ]
            .into(),
            paths: brie_cfg::Paths::default(),
        };

        download_all(cache_dir, &config).unwrap();

        // FIXME add actual assertions
    }
}
