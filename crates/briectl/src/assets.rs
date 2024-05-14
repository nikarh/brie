use std::{
    borrow::Cow,
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
};

use brie_cfg::Brie;
use brie_download::{download_file, mp, ureq, TlsError};
use image::{GenericImageView, ImageFormat};
use indicatif::{ProgressBar, ProgressFinish, ProgressStyle};
use log::{debug, error, info, warn};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("TLS error. {0}")]
    Tls(#[from] &'static TlsError),
    #[error("Download error. {0}")]
    Download(#[from] brie_download::Error),
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

    let res: Container<Vec<AutocompleteResponse>> = ureq()?
        .request_url("GET", &url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(Box::new)?
        .into_json()?;

    Ok(res.data.first().map(|r| r.id))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ImageKind {
    Grid,
    Icon,
    Hero,
    Logo,
}

impl ImageKind {
    pub fn all() -> [ImageKind; 4] {
        [
            ImageKind::Grid,
            ImageKind::Icon,
            ImageKind::Hero,
            ImageKind::Logo,
        ]
    }

    fn path(self) -> &'static str {
        match self {
            ImageKind::Grid => "grids",
            ImageKind::Icon => "icons",
            ImageKind::Hero => "heroes",
            ImageKind::Logo => "logos",
        }
    }

    fn require_png(self) -> bool {
        matches!(self, Self::Grid | Self::Icon)
    }
}

impl std::fmt::Display for ImageKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ImageKind::Grid => "grid",
            ImageKind::Icon => "icon",
            ImageKind::Hero => "hero",
            ImageKind::Logo => "logo",
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
            ImageKind::Icon | ImageKind::Hero | ImageKind::Logo => {
                images.first().map(|img| img.thumb.as_str())
            }
        }
    }
}

#[derive(Deserialize)]
struct ImageResponse {
    url: String,
    thumb: String,
    width: u32,
}

fn image(token: &str, kind: ImageKind, id: u32, name: &str) -> Result<Option<Vec<u8>>, Error> {
    info!("Downloading and re-encoding `{kind}` image for {id} ({name})");

    let url = format!(
        "https://www.steamgriddb.com/api/v2/{kind}/game/{id}",
        kind = kind.path()
    );

    let res: Container<Vec<ImageResponse>> = ureq()?
        .get(&url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(Box::new)?
        .into_json()?;

    let Some(url) = kind.filter(&res.data) else {
        return Ok(None);
    };

    let (mut lib, pb) = download_file(url, None)?.progress(format!("{id}-{kind}"));

    let mut img = Vec::new();
    lib.read_to_end(&mut img)?;
    pb.finish();

    if kind.require_png() {
        let pb = mp().add(
            ProgressBar::new_spinner()
                .with_message(format!("Converting {id}-{kind} ({name}) to png"))
                .with_finish(ProgressFinish::AndLeave)
                .with_style(
                    ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] {msg:>15}")
                        .unwrap(),
                ),
        );
        img = convert_to_png(&img)?;
        pb.finish_with_message(format!("Converted {id}-{kind} to png"));
    }

    Ok(Some(img))
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

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
pub struct Images(HashMap<ImageKind, PathBuf>);

impl Images {
    pub fn get(&self, kind: ImageKind) -> Option<&Path> {
        self.0.get(&kind).map(PathBuf::as_path)
    }
}

#[derive(Default, Serialize, Deserialize)]
struct CachedAssets {
    ids: HashMap<String, Option<u32>>,
    images: HashMap<u32, Images>,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Assets {
    ids: HashMap<String, u32>,
    images: HashMap<u32, Images>,
}

impl Assets {
    pub fn get_all(&self, name: &str) -> Cow<'_, Images> {
        let Some(id) = self.ids.get(name) else {
            return Cow::Owned(Images::default());
        };

        match self.images.get(id) {
            Some(images) => Cow::Borrowed(images),
            None => Cow::Owned(Images::default()),
        }
    }

    pub fn get(&self, name: &str, kind: ImageKind) -> Option<&Path> {
        let id = self.ids.get(name)?;

        self.images
            .get(id)
            .and_then(|i| i.0.get(&kind))
            .map(PathBuf::as_path)
    }
}

fn ensure_steamgriddb_ids(
    assets: &mut CachedAssets,
    token: &str,
    config: &Brie,
) -> HashMap<String, u32> {
    info!("Finding missing steamgriddb ids");

    // Find ids in steamgriddb for units missing it. Ideally it should append it to `brie.yaml`, but
    // that might be complicated, considering formatting and comments should remain intact.
    let found_ids = config
        .units
        .par_iter()
        .map(|(k, v)| (k, v.common()))
        .filter(|(k, v)| !assets.ids.contains_key(*k) && v.steamgriddb_id.is_none())
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
        .map(|(k, id)| (k.to_owned(), id))
        .collect::<HashMap<_, _>>();

    if !found_ids.is_empty() {
        debug!("Found ids: {found_ids:?}");
        assets.ids.extend(found_ids);
    }

    // Merge cached ids with ids defined in the unit file
    let cached_ids = assets
        .ids
        .iter()
        .filter_map(|(k, v)| v.map(|v| (k.clone(), v)));

    let mut predefined_ids = config
        .units
        .iter()
        .map(|(k, v)| (k, v.common()))
        .filter_map(|(k, v)| v.steamgriddb_id.map(|id| (k.to_owned(), id)))
        .collect::<HashMap<_, _>>();

    predefined_ids.extend(cached_ids);
    predefined_ids
}

fn ensure_images_exist(
    assets: &mut CachedAssets,
    id_map: &HashMap<String, u32>,
    token: &str,
    cache_dir: &Path,
) {
    let _ = std::fs::create_dir_all(cache_dir.join("images"));

    let ids = id_map
        .iter()
        .map(|(name, id)| (id, (name, assets.images.get(id))))
        .collect::<HashMap<_, _>>();
    let paths = ids
        .par_iter()
        .flat_map(|(&&id, &name)| ImageKind::all().map(|kind| (id, name, kind)))
        .into_par_iter()
        .filter_map(|(id, (name, cache), kind)| {
            if let Some(cached) = cache.and_then(|c| c.0.get(&kind)) {
                if cached.exists() {
                    return Some(((id, kind), cached.clone()));
                }
            }

            let path = cache_dir.join("images").join(format!("{id}-{kind}"));
            match image(token, kind, id, name) {
                Ok(Some(img)) => {
                    let ext = match image::guess_format(&img) {
                        Ok(ImageFormat::Jpeg) => "jpg",
                        Ok(ImageFormat::Png) => "png",
                        // TODO: handle this case
                        format => {
                            warn!("Unknown image format: {format:?}");
                            "png"
                        }
                    };

                    let path = path.with_extension(ext);

                    // TODO: error handling?
                    let _ = std::fs::write(&path, img);
                    Some(((id, kind), path))
                }
                Ok(None) => {
                    warn!("No `{kind}` image found for id {id} ({name})");
                    None
                }
                Err(e) => {
                    error!("Failed to download `{kind}` image for id {id} ({name}): {e}");
                    None
                }
            }
        })
        .collect::<HashMap<_, _>>();

    for ((id, kind), path) in paths {
        assets.images.entry(id).or_default().0.insert(kind, path);
    }
}

pub fn download_all(cache_dir: &Path, config: &Brie) -> Result<Assets, Error> {
    info!("Downloading banners and icons from steamgriddb");
    let _ = std::fs::create_dir_all(cache_dir);

    let asset_cache = cache_dir.join("assets.json");
    let mut assets: CachedAssets = std::fs::read(&asset_cache)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let Some(token) = config.tokens.as_ref().and_then(|t| t.steamgriddb.as_ref()) else {
        warn!("steamgriddb_token is not defined in the config");
        return Ok(Assets {
            ids: assets
                .ids
                .into_iter()
                .filter_map(|(k, v)| v.map(|v| (k, v)))
                .collect(),
            images: assets.images,
        });
    };

    let id_map = ensure_steamgriddb_ids(&mut assets, token, config);
    ensure_images_exist(&mut assets, &id_map, token, cache_dir);

    let cached_ids = serde_json::to_vec(&assets)?;
    std::fs::write(&asset_cache, cached_ids)?;

    Ok(Assets {
        ids: id_map,
        images: assets.images,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use brie_cfg::Tokens;
    use brie_download::mp;
    use indicatif_log_bridge::LogWrapper;

    use crate::assets::ImageKind;

    use super::{autocomplete, download_all, image};

    const TOKEN: &str = "82e919fd236407ddbf5012fdb1b13126";

    #[test]
    pub fn test_autocomplete() {
        let res = autocomplete(TOKEN, "The witcher 3").unwrap();
        assert_eq!(res, Some(4265));
    }

    #[test]
    #[ignore]
    pub fn test_banners() {
        let res = image(TOKEN, ImageKind::Grid, 4265, "game")
            .unwrap()
            .unwrap();
        assert!(res == std::fs::read("tests/grid.png").unwrap());
        let res = image(TOKEN, ImageKind::Icon, 4265, "game")
            .unwrap()
            .unwrap();
        assert!(res == std::fs::read("tests/icon.png").unwrap());
    }

    #[test]
    pub fn test_download_all() {
        let log = simple_logger::SimpleLogger::new()
            .with_level(log::LevelFilter::Info)
            .env();
        LogWrapper::new(mp().clone(), log).try_init().unwrap();

        let cache_dir = Path::new(".tmp/cache");
        let config = brie_cfg::Brie {
            tokens: Some(Tokens {
                steamgriddb: Some(TOKEN.to_owned()),
                github: None,
            }),
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
