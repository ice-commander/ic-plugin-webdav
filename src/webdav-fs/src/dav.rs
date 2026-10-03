pub struct Config {
    pub url: String,
    pub user: Option<String>,
    pub pass: Option<String>,
}

pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: u64,
}

fn percent_decode(text: &str) -> String {
    let mut bytes = Vec::new();
    let mut source = text.as_bytes().iter();
    while let Some(&c) = source.next() {
        if c == b'%' {
            if let (Some(&high), Some(&low)) = (source.next(), source.next()) {
                if let Ok(pair) = String::from_utf8(vec![high, low]) {
                    if let Ok(byte) = u8::from_str_radix(&pair, 16) {
                        bytes.push(byte);
                        continue;
                    }
                }
            }
        }
        bytes.push(c);
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn tag_content(segment: &str, tag: &str) -> Option<String> {
    let opening = format!("<{tag}");
    let closing = format!("</{tag}>");
    let start = segment.find(&opening)?;
    let after = start + segment[start..].find('>')? + 1;
    let end = after + segment[after..].find(&closing)?;
    Some(segment[after..end].trim().to_string())
}

fn seconds_of(raw: &str) -> u64 {
    if raw.is_empty() {
        return 0;
    }
    chrono::DateTime::parse_from_rfc2822(raw)
        .map(|stamp| stamp.timestamp().max(0) as u64)
        .unwrap_or(0)
}

impl Config {
    pub fn address_of(&self, path: &str) -> String {
        let mut url = self.url.trim_end_matches('/').to_string();
        let wanted = path.trim_start_matches(['/', '\\']);
        if !wanted.is_empty() {
            url.push('/');
            url.push_str(&wanted.replace('\\', "/"));
        }
        url
    }
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .pool_max_idle_per_host(10)
        .build()
        .map_err(|e| e.to_string())
}

fn authorised(
    request: reqwest::blocking::RequestBuilder,
    cfg: &Config,
) -> reqwest::blocking::RequestBuilder {
    match (&cfg.user, &cfg.pass) {
        (Some(user), Some(pass)) if !user.is_empty() => request.basic_auth(user, Some(pass)),
        (Some(user), None) if !user.is_empty() => request.basic_auth(user, None::<&str>),
        _ => request,
    }
}

fn method(name: &[u8]) -> Result<reqwest::Method, String> {
    reqwest::Method::from_bytes(name).map_err(|e| e.to_string())
}

pub fn list(cfg: &Config, path: &str) -> Result<Vec<Entry>, String> {
    let url = cfg.address_of(path);
    let response = authorised(
        client()?
            .request(method(b"PROPFIND")?, &url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml"),
        cfg,
    )
    .send()
    .map_err(|e| format!("PROPFIND failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("the server answered {status}"));
    }
    let body = response
        .text()
        .map_err(|e| format!("could not read the answer: {e}"))?
        .replace("<D:", "<")
        .replace("<d:", "<")
        .replace("</D:", "</")
        .replace("</d:", "</");

    let here = reqwest::Url::parse(&url)
        .map(|parsed| percent_decode(parsed.path()))
        .unwrap_or_else(|_| percent_decode(&url));
    let here = here.trim_end_matches('/').to_string();

    let mut found = Vec::new();
    for segment in body.split("</response>") {
        if !segment.contains("<href") {
            continue;
        }
        let Some(href) = tag_content(segment, "href") else {
            continue;
        };
        if href.is_empty() {
            continue;
        }
        let decoded = percent_decode(&href);
        let trimmed = decoded.trim_end_matches('/');
        if trimmed == here {
            continue;
        }
        let name = trimmed.rsplit('/').next().unwrap_or_default().to_string();
        if name.is_empty() {
            continue;
        }
        let is_dir = segment.contains("<collection>")
            || segment.contains("<collection/>")
            || segment.contains("<collection ");
        let modified = seconds_of(&tag_content(segment, "getlastmodified").unwrap_or_default());
        let size = if is_dir {
            0
        } else {
            tag_content(segment, "getcontentlength")
                .and_then(|text| text.parse().ok())
                .unwrap_or(0)
        };
        found.push(Entry {
            name,
            is_dir,
            size,
            modified,
        });
    }
    Ok(found)
}

pub fn read(cfg: &Config, path: &str) -> Result<Vec<u8>, String> {
    let url = cfg.address_of(path);
    let response = authorised(client()?.get(&url), cfg)
        .send()
        .map_err(|e| format!("GET failed: {e}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("the server answered {status}"));
    }
    response
        .bytes()
        .map(|body| body.to_vec())
        .map_err(|e| format!("could not read the file: {e}"))
}

pub fn write(cfg: &Config, path: &str, bytes: Vec<u8>) -> Result<(), String> {
    let url = cfg.address_of(path);
    let response = authorised(client()?.put(&url).body(bytes), cfg)
        .send()
        .map_err(|e| format!("PUT failed: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("the server answered {status}"))
    }
}

pub fn create_dir(cfg: &Config, path: &str) -> Result<(), String> {
    let url = cfg.address_of(path);
    let response = authorised(client()?.request(method(b"MKCOL")?, &url), cfg)
        .send()
        .map_err(|e| format!("MKCOL failed: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("the server answered {status}"))
    }
}

pub fn remove(cfg: &Config, path: &str) -> Result<(), String> {
    let url = cfg.address_of(path);
    let response = authorised(client()?.delete(&url), cfg)
        .send()
        .map_err(|e| format!("DELETE failed: {e}"))?;
    let status = response.status();
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!("the server answered {status}"))
    }
}

pub fn rename(cfg: &Config, from: &str, to: &str) -> Result<(), String> {
    let source = cfg.address_of(from);
    let target = cfg.address_of(to);
    let response = authorised(
        client()?
            .request(method(b"MOVE")?, &source)
            .header("Destination", target)
            .header("Overwrite", "T"),
        cfg,
    )
    .send()
    .map_err(|e| format!("MOVE failed: {e}"))?;
    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        Err(format!("the server answered {status}"))
    }
}
