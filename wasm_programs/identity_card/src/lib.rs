use base64::Engine;
use blake3::Hasher;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Register {
        name: String,
        email: String,
        bio: Option<String>,
        avatar_url: Option<String>,
        theme: Option<String>,
    },
    GetProfile {
        id: String,
    },
    UpdateProfile {
        id: String,
        name: Option<String>,
        email: Option<String>,
        bio: Option<String>,
        avatar_url: Option<String>,
        theme: Option<String>,
    },
    GetCard {
        id: String,
        format: Option<String>,
    },
    ListAll,
    Stats,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Response {
    OkRegister {
        id: String,
        identity: Identity,
        card_svg: String,
    },
    OkProfile {
        identity: Identity,
    },
    OkUpdate {
        id: String,
        identity: Identity,
    },
    OkCard {
        id: String,
        format: String,
        card_svg: String,
    },
    OkList {
        identities: Vec<IdentitySummary>,
        total: usize,
    },
    OkStats {
        total_identities: usize,
        total_storage_bytes: usize,
        merkle_root: String,
    },
    Err {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Identity {
    id: String,
    name: String,
    email: String,
    bio: Option<String>,
    avatar_url: Option<String>,
    theme: String,
    created_at: u64,
    updated_at: u64,
    checksum: String,
}

#[derive(Debug, Serialize)]
struct IdentitySummary {
    id: String,
    name: String,
    email: String,
    theme: String,
    created_at: u64,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let response = match handle_request(input) {
        Ok(resp) => resp,
        Err(msg) => Response::Err { message: msg },
    };

    let encoded = serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"status\":\"err\",\"message\":\"serialize\"}".to_vec());
    let buf = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(64 * 1024)));
    let mut guard = buf.lock().unwrap();
    guard.clear();
    guard.extend_from_slice(&encoded);
    let out_ptr = guard.as_ptr() as i32;
    let out_len = guard.len() as i32;
    std::mem::forget(guard);
    (out_ptr, out_len)
}

#[no_mangle]
pub extern "C" fn onvm_last_result() -> (i32, i32) {
    if let Some(buf) = OUT_BUF.get() {
        let guard = buf.lock().unwrap();
        let ptr = guard.as_ptr() as i32;
        let len = guard.len() as i32;
        std::mem::forget(guard);
        (ptr, len)
    } else {
        (0, 0)
    }
}

fn handle_request(input: &[u8]) -> Result<Response, String> {
    let req: Request =
        serde_json::from_slice(input).map_err(|e| format!("invalid json: {e}"))?;

    match req {
        Request::Register {
            name,
            email,
            bio,
            avatar_url,
            theme,
        } => {
            validate_email(&email)?;
            validate_name(&name)?;

            let id = generate_identity_id(&name, &email);
            
            if identity_exists(&id)? {
                return Err(format!("identity already exists: {}", id));
            }

            let now = current_timestamp();
            let theme_final = theme.unwrap_or_else(|| "blue".to_string());

            let identity = Identity {
                id: id.clone(),
                name: name.clone(),
                email: email.clone(),
                bio,
                avatar_url,
                theme: theme_final,
                created_at: now,
                updated_at: now,
                checksum: String::new(),
            };

            let identity_with_checksum = calculate_identity_checksum(identity);
            save_identity(&identity_with_checksum)?;
            add_to_identity_index(&id)?;

            let card_svg = generate_card_svg(&identity_with_checksum);

            Ok(Response::OkRegister {
                id,
                identity: identity_with_checksum,
                card_svg,
            })
        }

        Request::GetProfile { id } => {
            let identity = load_identity(&id)?;
            Ok(Response::OkProfile { identity })
        }

        Request::UpdateProfile {
            id,
            name,
            email,
            bio,
            avatar_url,
            theme,
        } => {
            let mut identity = load_identity(&id)?;

            if let Some(n) = name {
                validate_name(&n)?;
                identity.name = n;
            }
            if let Some(e) = email {
                validate_email(&e)?;
                identity.email = e;
            }
            if let Some(b) = bio {
                identity.bio = Some(b);
            }
            if let Some(a) = avatar_url {
                identity.avatar_url = Some(a);
            }
            if let Some(t) = theme {
                identity.theme = t;
            }

            identity.updated_at = current_timestamp();
            let identity_with_checksum = calculate_identity_checksum(identity);
            save_identity(&identity_with_checksum)?;

            Ok(Response::OkUpdate {
                id,
                identity: identity_with_checksum,
            })
        }

        Request::GetCard { id, format } => {
            let identity = load_identity(&id)?;
            let fmt = format.unwrap_or_else(|| "svg".to_string());

            let card_svg = generate_card_svg(&identity);

            Ok(Response::OkCard {
                id,
                format: fmt,
                card_svg,
            })
        }

        Request::ListAll => {
            let ids = load_identity_index()?;
            let mut identities = Vec::new();

            for id in &ids {
                if let Ok(identity) = load_identity(id) {
                    identities.push(IdentitySummary {
                        id: identity.id,
                        name: identity.name,
                        email: identity.email,
                        theme: identity.theme,
                        created_at: identity.created_at,
                    });
                }
            }

            identities.sort_by(|a, b| b.created_at.cmp(&a.created_at));

            Ok(Response::OkList {
                total: identities.len(),
                identities,
            })
        }

        Request::Stats => {
            let ids = load_identity_index()?;
            let mut total_bytes = 0usize;
            let mut combined = Vec::new();

            for id in &ids {
                let key = identity_key(id);
                if let Ok(data) = state_get_with_resize(key.as_bytes()) {
                    total_bytes += data.len();
                    combined.extend_from_slice(&data);
                }
            }

            let merkle_root = blake3_hex(&combined);

            Ok(Response::OkStats {
                total_identities: ids.len(),
                total_storage_bytes: total_bytes,
                merkle_root,
            })
        }
    }
}

fn generate_identity_id(name: &str, email: &str) -> String {
    let mut hasher = Hasher::new();
    hasher.update(name.as_bytes());
    hasher.update(email.as_bytes());
    hasher.update(&current_timestamp().to_le_bytes());
    let hash = hasher.finalize();
    hex::encode(&hash.as_bytes()[..16])
}

fn validate_email(email: &str) -> Result<(), String> {
    if email.is_empty() || !email.contains('@') || email.len() > 320 {
        return Err("invalid email format".to_string());
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 100 {
        return Err("name must be 1-100 characters".to_string());
    }
    Ok(())
}

fn current_timestamp() -> u64 {
    static mut COUNTER: u64 = 0;
    unsafe {
        COUNTER += 1;
        let base = 1734000000;
        base + COUNTER
    }
}

fn calculate_identity_checksum(mut identity: Identity) -> Identity {
    let mut hasher = Hasher::new();
    hasher.update(identity.id.as_bytes());
    hasher.update(identity.name.as_bytes());
    hasher.update(identity.email.as_bytes());
    if let Some(ref bio) = identity.bio {
        hasher.update(bio.as_bytes());
    }
    if let Some(ref avatar) = identity.avatar_url {
        hasher.update(avatar.as_bytes());
    }
    hasher.update(identity.theme.as_bytes());
    hasher.update(&identity.created_at.to_le_bytes());
    hasher.update(&identity.updated_at.to_le_bytes());

    identity.checksum = hasher.finalize().to_hex().to_string();
    identity
}

fn identity_key(id: &str) -> String {
    format!("identity:{}", id)
}

fn identity_exists(id: &str) -> Result<bool, String> {
    let key = identity_key(id);
    match state_get_with_resize(key.as_bytes()) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

fn save_identity(identity: &Identity) -> Result<(), String> {
    let key = identity_key(&identity.id);
    let data =
        serde_json::to_vec(identity).map_err(|e| format!("serialize identity: {e}"))?;
    state_put(key.as_bytes(), &data)?;
    Ok(())
}

fn load_identity(id: &str) -> Result<Identity, String> {
    let key = identity_key(id);
    let data = state_get_with_resize(key.as_bytes())?;
    let identity: Identity =
        serde_json::from_slice(&data).map_err(|e| format!("deserialize identity: {e}"))?;
    Ok(identity)
}

const IDENTITY_INDEX_KEY: &str = "__identity_index";

fn load_identity_index() -> Result<Vec<String>, String> {
    match state_get_with_resize(IDENTITY_INDEX_KEY.as_bytes()) {
        Ok(bytes) => {
            if bytes.is_empty() {
                return Ok(Vec::new());
            }
            let ids: Vec<String> = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize index: {e}"))?;
            Ok(ids)
        }
        Err(_) => Ok(Vec::new()),
    }
}

fn add_to_identity_index(id: &str) -> Result<(), String> {
    let mut ids = load_identity_index()?;
    if !ids.contains(&id.to_string()) {
        ids.push(id.to_string());
        let data =
            serde_json::to_vec(&ids).map_err(|e| format!("serialize index: {e}"))?;
        state_put(IDENTITY_INDEX_KEY.as_bytes(), &data)?;
    }
    Ok(())
}

fn generate_card_svg(identity: &Identity) -> String {
    let (bg_color, text_color, accent_color) = get_theme_colors(&identity.theme);
    
    let bio_text = identity
        .bio
        .as_ref()
        .map(|b| truncate_text(b, 60))
        .unwrap_or_else(|| "No bio provided".to_string());

    let avatar_element = if let Some(ref url) = identity.avatar_url {
        format!(
            r#"<image x="20" y="20" width="80" height="80" href="{}" clip-path="url(#avatar-clip)" />"#,
            escape_xml(url)
        )
    } else {
        format!(
            r#"<rect x="20" y="20" width="80" height="80" rx="40" fill="{}" />
            <text x="60" y="70" font-family="Arial, sans-serif" font-size="36" fill="{}" text-anchor="middle">{}</text>"#,
            accent_color,
            bg_color,
            identity.name.chars().next().unwrap_or('?').to_uppercase()
        )
    };

    let id_short = if identity.id.len() > 12 {
        format!("{}...{}", &identity.id[..6], &identity.id[identity.id.len()-6..])
    } else {
        identity.id.clone()
    };

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 250" width="400" height="250">
  <defs>
    <linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" style="stop-color:{};stop-opacity:1" />
      <stop offset="100%" style="stop-color:{};stop-opacity:1" />
    </linearGradient>
    <clipPath id="avatar-clip">
      <circle cx="60" cy="60" r="40" />
    </clipPath>
  </defs>
  
  <rect width="400" height="250" fill="url(#bg-gradient)" rx="15" />
  
  <rect x="10" y="10" width="380" height="230" fill="none" stroke="{}" stroke-width="2" rx="12" opacity="0.3" />
  
  {}
  
  <text x="120" y="45" font-family="Arial, sans-serif" font-size="24" font-weight="bold" fill="{}">{}</text>
  <text x="120" y="70" font-family="Arial, sans-serif" font-size="14" fill="{}" opacity="0.9">{}</text>
  <text x="120" y="90" font-family="Arial, sans-serif" font-size="11" fill="{}" opacity="0.7">ID: {}</text>
  
  <rect x="20" y="120" width="360" height="1" fill="{}" opacity="0.3" />
  
  <text x="20" y="145" font-family="Arial, sans-serif" font-size="12" font-weight="bold" fill="{}" opacity="0.8">BIO</text>
  <text x="20" y="165" font-family="Arial, sans-serif" font-size="11" fill="{}">{}</text>
  
  <text x="20" y="200" font-family="Arial, sans-serif" font-size="9" fill="{}" opacity="0.6">Created: {}</text>
  <text x="20" y="215" font-family="Arial, sans-serif" font-size="9" fill="{}" opacity="0.6">Checksum: {}...</text>
  
  <circle cx="375" cy="225" r="8" fill="{}" opacity="0.5" />
  <text x="375" y="230" font-family="Arial, sans-serif" font-size="10" fill="{}" text-anchor="middle">✓</text>
</svg>"#,
        bg_color,
        darken_color(&bg_color),
        accent_color,
        avatar_element,
        text_color,
        escape_xml(&identity.name),
        text_color,
        escape_xml(&identity.email),
        text_color,
        escape_xml(&id_short),
        text_color,
        text_color,
        text_color,
        escape_xml(&bio_text),
        text_color,
        format_timestamp(identity.created_at),
        text_color,
        &identity.checksum[..12],
        accent_color,
        text_color,
    )
}

fn get_theme_colors(theme: &str) -> (&'static str, &'static str, &'static str) {
    match theme.to_lowercase().as_str() {
        "blue" => ("#1e3a8a", "#ffffff", "#60a5fa"),
        "purple" => ("#581c87", "#ffffff", "#a78bfa"),
        "green" => ("#14532d", "#ffffff", "#4ade80"),
        "red" => ("#7f1d1d", "#ffffff", "#f87171"),
        "orange" => ("#7c2d12", "#ffffff", "#fb923c"),
        "pink" => ("#831843", "#ffffff", "#f472b6"),
        "dark" => ("#0f172a", "#ffffff", "#64748b"),
        "light" => ("#f8fafc", "#0f172a", "#3b82f6"),
        _ => ("#1e3a8a", "#ffffff", "#60a5fa"),
    }
}

fn darken_color(color: &str) -> String {
    if color.starts_with('#') && color.len() == 7 {
        let r = u8::from_str_radix(&color[1..3], 16).unwrap_or(0);
        let g = u8::from_str_radix(&color[3..5], 16).unwrap_or(0);
        let b = u8::from_str_radix(&color[5..7], 16).unwrap_or(0);
        
        let r = (r as f32 * 0.7) as u8;
        let g = (g as f32 * 0.7) as u8;
        let b = (b as f32 * 0.7) as u8;
        
        format!("#{:02x}{:02x}{:02x}", r, g, b)
    } else {
        color.to_string()
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len - 3])
    }
}

fn format_timestamp(ts: u64) -> String {
    let years_since_2023 = ts.saturating_sub(1672531200) / 31536000;
    
    if years_since_2023 == 0 {
        "Recently".to_string()
    } else if years_since_2023 == 1 {
        "1 year ago".to_string()
    } else {
        format!("{} years ago", years_since_2023)
    }
}

fn blake3_hex(data: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize().to_hex().to_string()
}

fn state_put(key: &[u8], value: &[u8]) -> Result<(), String> {
    let rc = unsafe {
        onvm_state_put(
            key.as_ptr() as i32,
            key.len() as i32,
            value.as_ptr() as i32,
            value.len() as i32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("state put failed: {rc}"))
    }
}

fn state_get_with_resize(key: &[u8]) -> Result<Vec<u8>, String> {
    let mut cap = 2048;
    for _ in 0..5 {
        let mut buf = vec![0u8; cap];
        let len = unsafe {
            onvm_state_get(
                key.as_ptr() as i32,
                key.len() as i32,
                buf.as_mut_ptr() as i32,
                cap as i32,
            )
        };
        if len == 0 {
            return Err("key not found".into());
        }
        if len > 0 {
            buf.truncate(len as usize);
            return Ok(buf);
        }
        cap = (-len) as usize;
    }
    Err("unable to fetch value".into())
}

mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

extern "C" {
    fn onvm_state_put(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
    fn onvm_state_get(key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32) -> i32;
}