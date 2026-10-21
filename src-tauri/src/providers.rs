//! Unsplash and Pexels API clients, normalised into a single `Photo` type.

use rand::seq::SliceRandom;
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const UNSPLASH_API: &str = "https://api.unsplash.com";
const PEXELS_API: &str = "https://api.pexels.com/v1";
const WALLHAVEN_API: &str = "https://wallhaven.cc/api/v1";
const BING_API: &str = "https://www.bing.com/HPImageArchive.aspx?format=js&n=8&mkt=en-US";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Unsplash,
    Pexels,
    /// Keyless. Search, random and top lists; SFW "general" category only.
    Wallhaven,
    /// Keyless. Bing's recent daily images; no search.
    Bing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: String,
    pub source: Source,
    /// Small image for grids.
    pub thumb: String,
    /// Medium image for previews and the blurred app backdrop.
    pub preview: String,
    /// Base URL for the full-resolution download (width is appended later).
    pub full: String,
    pub width: u32,
    pub height: u32,
    pub author: String,
    pub author_url: String,
    pub page_url: String,
    /// Dominant colour reported by the provider, `#rrggbb`.
    pub color: String,
    pub description: String,
    /// Unsplash requires hitting this endpoint when a photo is used.
    #[serde(default)]
    pub download_location: Option<String>,
}

impl Photo {
    /// URL sized for the current display.
    pub fn download_url(&self, screen_width: u32) -> String {
        let w = screen_width.clamp(1920, 7680);
        match self.source {
            Source::Unsplash => format!("{}&w={w}&fm=jpg&q=88&fit=max", self.full),
            Source::Pexels => format!("{}?auto=compress&cs=tinysrgb&fit=max&w={w}", self.full),
            Source::Wallhaven | Source::Bing => self.full.clone(),
        }
    }

    pub fn extension(&self) -> &'static str {
        if self.full.to_lowercase().ends_with(".png") {
            "png"
        } else {
            "jpg"
        }
    }

    pub fn key(&self) -> String {
        format!("{:?}_{}", self.source, self.id).to_lowercase()
    }
}

#[derive(Clone, Default)]
pub struct Keys {
    pub unsplash: Option<String>,
    pub pexels: Option<String>,
}

impl Keys {
    pub fn has(&self, s: Source) -> bool {
        match s {
            Source::Unsplash => self.unsplash.is_some(),
            Source::Pexels => self.pexels.is_some(),
            Source::Wallhaven | Source::Bing => true,
        }
    }
}

fn s(v: &Value, path: &[&str]) -> String {
    let mut cur = v;
    for p in path {
        cur = &cur[*p];
    }
    match cur {
        Value::String(x) => x.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn u(v: &Value, key: &str) -> u32 {
    v[key].as_u64().unwrap_or(0) as u32
}

fn unsplash_photo(v: &Value) -> Photo {
    let desc = {
        let d = s(v, &["alt_description"]);
        if d.is_empty() {
            s(v, &["description"])
        } else {
            d
        }
    };
    Photo {
        id: s(v, &["id"]),
        source: Source::Unsplash,
        thumb: s(v, &["urls", "small"]),
        preview: s(v, &["urls", "regular"]),
        full: s(v, &["urls", "raw"]),
        width: u(v, "width"),
        height: u(v, "height"),
        author: s(v, &["user", "name"]),
        author_url: format!(
            "{}?utm_source=wallflow&utm_medium=referral",
            s(v, &["user", "links", "html"])
        ),
        page_url: format!(
            "{}?utm_source=wallflow&utm_medium=referral",
            s(v, &["links", "html"])
        ),
        color: s(v, &["color"]),
        description: desc,
        download_location: Some(s(v, &["links", "download_location"])).filter(|x| !x.is_empty()),
    }
}

fn pexels_photo(v: &Value) -> Photo {
    Photo {
        id: s(v, &["id"]),
        source: Source::Pexels,
        thumb: s(v, &["src", "large"]),
        preview: s(v, &["src", "large2x"]),
        full: s(v, &["src", "original"]),
        width: u(v, "width"),
        height: u(v, "height"),
        author: s(v, &["photographer"]),
        author_url: s(v, &["photographer_url"]),
        page_url: s(v, &["url"]),
        color: s(v, &["avg_color"]),
        description: s(v, &["alt"]),
        download_location: None,
    }
}

fn wallhaven_photo(v: &Value) -> Photo {
    let id = s(v, &["id"]);
    let res = s(v, &["resolution"]);
    Photo {
        source: Source::Wallhaven,
        thumb: s(v, &["thumbs", "large"]),
        preview: s(v, &["path"]),
        full: s(v, &["path"]),
        width: u(v, "dimension_x"),
        height: u(v, "dimension_y"),
        author: "Wallhaven".into(),
        author_url: s(v, &["url"]),
        page_url: s(v, &["url"]),
        color: v["colors"]
            .get(0)
            .and_then(|c| c.as_str())
            .unwrap_or("#808080")
            .into(),
        description: format!("Wallhaven wallpaper · {}", res.replace('x', " × ")),
        download_location: None,
        id,
    }
}

fn bing_photo(v: &Value) -> Photo {
    let base = format!("https://www.bing.com{}", s(v, &["urlbase"]));
    let copyright = s(v, &["copyright"]);
    // "Place, Country (© Author/Agency)" → author = text inside "(© …)"
    let author = copyright
        .rsplit_once("(©")
        .map(|(_, a)| a.trim_end_matches(')').trim().to_string())
        .unwrap_or_else(|| "Bing".into());
    let title = s(v, &["title"]);
    Photo {
        id: s(v, &["startdate"]),
        source: Source::Bing,
        thumb: format!("{base}_800x480.jpg"),
        preview: format!("{base}_1920x1080.jpg"),
        full: format!("{base}_UHD.jpg"),
        width: 3840,
        height: 2160,
        author,
        author_url: s(v, &["copyrightlink"]),
        page_url: s(v, &["copyrightlink"]),
        color: String::new(),
        description: if title.is_empty() { copyright } else { title },
        download_location: None,
    }
}

async fn wallhaven(client: &Client, params: &str) -> Result<Vec<Photo>, String> {
    let url = format!(
        "{WALLHAVEN_API}/search?categories=100&purity=100&atleast=1920x1080&ratios=landscape&{params}"
    );
    let v = get_json(client, &url, None).await?;
    Ok(v["data"]
        .as_array()
        .map(|a| a.iter().map(wallhaven_photo).collect())
        .unwrap_or_default())
}

async fn bing(client: &Client) -> Result<Vec<Photo>, String> {
    let v = get_json(client, &format!("{BING_API}&idx=0"), None).await?;
    Ok(v["images"]
        .as_array()
        .map(|a| a.iter().map(bing_photo).collect())
        .unwrap_or_default())
}

async fn get_json(client: &Client, url: &str, auth: Option<String>) -> Result<Value, String> {
    let mut req = client.get(url).header("Accept-Version", "v1");
    if let Some(auth) = auth {
        req = req.header("Authorization", auth);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        let hint = match status.as_u16() {
            401 | 403 => " (check your API key)",
            429 => " (rate limit reached, try again later)",
            _ => "",
        };
        return Err(format!(
            "API error {status}{hint}: {}",
            body.chars().take(160).collect::<String>()
        ));
    }
    resp.json().await.map_err(|e| format!("Bad response: {e}"))
}

fn enc(q: &str) -> String {
    q.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Fetch one page from a single source. Empty query = its recommended feed.
async fn fetch(
    client: &Client,
    keys: &Keys,
    src: Source,
    q: &str,
    page: u32,
    per_page: u32,
) -> Result<Vec<Photo>, String> {
    match src {
        Source::Unsplash => {
            let key = keys.unsplash.as_ref().ok_or("Unsplash needs an API key")?;
            let url = if q.is_empty() {
                format!("{UNSPLASH_API}/topics/wallpapers/photos?per_page={per_page}&page={page}&orientation=landscape")
            } else {
                format!(
                    "{UNSPLASH_API}/search/photos?query={}&per_page={per_page}&page={page}&orientation=landscape&content_filter=high",
                    enc(q)
                )
            };
            let v = get_json(client, &url, Some(format!("Client-ID {key}"))).await?;
            let arr = if q.is_empty() { &v } else { &v["results"] };
            Ok(arr
                .as_array()
                .map(|a| a.iter().map(unsplash_photo).collect())
                .unwrap_or_default())
        }
        Source::Pexels => {
            let key = keys.pexels.as_ref().ok_or("Pexels needs an API key")?;
            let url = if q.is_empty() {
                format!("{PEXELS_API}/curated?per_page={per_page}&page={page}")
            } else {
                format!("{PEXELS_API}/search?query={}&per_page={per_page}&page={page}&orientation=landscape", enc(q))
            };
            let v = get_json(client, &url, Some(key.clone())).await?;
            Ok(v["photos"]
                .as_array()
                .map(|a| a.iter().map(pexels_photo).collect())
                .unwrap_or_default())
        }
        Source::Wallhaven => {
            if q.is_empty() {
                wallhaven(client, &format!("sorting=toplist&topRange=1M&page={page}")).await
            } else {
                wallhaven(
                    client,
                    &format!("q={}&sorting=relevance&page={page}", enc(q)),
                )
                .await
            }
        }
        // Bing only has the last few daily images: one page, no search.
        Source::Bing if q.is_empty() && page == 1 => bing(client).await,
        Source::Bing => Ok(Vec::new()),
    }
}

/// Browse photos across `sources` (or just `only`), interleaving the results.
pub async fn browse(
    client: &Client,
    keys: &Keys,
    sources: &[Source],
    only: Option<Source>,
    query: &str,
    page: u32,
) -> Result<Vec<Photo>, String> {
    let sources: Vec<Source> = match only {
        Some(s) => vec![s],
        None => sources.iter().copied().filter(|s| keys.has(*s)).collect(),
    };
    if sources.is_empty() {
        return Err("No sources enabled. Turn one on under Rotation → Sources.".into());
    }
    let per_page = if sources.len() > 1 { 15 } else { 30 };
    let q = query.trim();

    let mut lists: Vec<Vec<Photo>> = Vec::new();
    let mut last_err = None;
    for src in sources {
        match fetch(client, keys, src, q, page, per_page).await {
            Ok(list) => lists.push(list),
            Err(e) => last_err = Some(e),
        }
    }
    if lists.is_empty() {
        return Err(last_err.unwrap_or_else(|| "No sources available".into()));
    }
    let mut out = Vec::new();
    let max = lists.iter().map(|l| l.len()).max().unwrap_or(0);
    for i in 0..max {
        for l in &lists {
            if let Some(p) = l.get(i) {
                out.push(p.clone());
            }
        }
    }
    Ok(out)
}

/// Pick a random landscape photo for one of the given categories.
/// Bing ignores categories, so it's only used when it's the sole source.
pub async fn random(
    client: &Client,
    keys: &Keys,
    enabled: &[Source],
    categories: &[String],
    avoid: &[String],
) -> Result<Photo, String> {
    let mut sources: Vec<Source> = enabled.iter().copied().filter(|s| keys.has(*s)).collect();
    if sources.len() > 1 {
        sources.retain(|s| *s != Source::Bing);
    }
    if sources.is_empty() {
        return Err("No sources enabled. Turn one on under Rotation → Sources.".into());
    }
    let mut last_err = String::new();
    for _ in 0..3 {
        let (src, cat) = {
            let mut rng = rand::thread_rng();
            let src = *sources.choose(&mut rng).unwrap();
            let cat = categories
                .choose(&mut rng)
                .cloned()
                .unwrap_or_else(|| "wallpaper".into());
            (src, cat)
        };
        let result = match src {
            Source::Unsplash => {
                let key = keys.unsplash.as_ref().unwrap();
                let url = format!(
                    "{UNSPLASH_API}/photos/random?query={}&orientation=landscape&content_filter=high&count=5",
                    enc(&cat)
                );
                get_json(client, &url, Some(format!("Client-ID {key}")))
                    .await
                    .map(|v| {
                        v.as_array()
                            .map(|a| a.iter().map(unsplash_photo).collect::<Vec<_>>())
                            .unwrap_or_default()
                    })
            }
            Source::Pexels => {
                let page = rand::thread_rng().gen_range(1..=8);
                fetch(client, keys, src, &cat, page, 40).await
            }
            Source::Wallhaven => {
                wallhaven(client, &format!("q={}&sorting=random", enc(&cat))).await
            }
            Source::Bing => bing(client).await,
        };
        match result {
            Ok(mut list) => {
                list.retain(|p| p.width >= p.height && !avoid.contains(&p.key()));
                if let Some(p) = list.choose(&mut rand::thread_rng()) {
                    return Ok(p.clone());
                }
                last_err = if src == Source::Bing {
                    "You've already used all of Bing's recent daily images".into()
                } else {
                    format!("No new photos found for \"{cat}\"")
                };
            }
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

/// Unsplash API guideline: trigger the download endpoint when a photo is used.
pub async fn track_download(client: &Client, keys: &Keys, photo: &Photo) {
    if let (Source::Unsplash, Some(loc), Some(key)) =
        (photo.source, &photo.download_location, &keys.unsplash)
    {
        let _ = client
            .get(loc)
            .header("Authorization", format!("Client-ID {key}"))
            .send()
            .await;
    }
}
