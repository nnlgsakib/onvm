use blake3::Hasher;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

// Shared buffer to return responses without heap allocations back to host.
static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Register {
        username: String,
        password: String,
    },
    UpdateProfile {
        session: String,
        display_name: Option<String>,
        bio: Option<String>,
        avatar_blob_id: Option<String>,
        banner_blob_id: Option<String>,
    },
    Login {
        username: String,
        password: String,
    },
    Logout {
        session: String,
    },
    CreatePost {
        session: String,
        text: String,
        attachments: Vec<String>,
    },
    Follow {
        session: String,
        target: String,
    },
    Unfollow {
        session: String,
        target: String,
    },
    DeletePost {
        session: String,
        post_id: String,
    },
    Like {
        session: String,
        post_id: String,
    },
    Comment {
        session: String,
        post_id: String,
        text: String,
    },
    Feed {
        session: String,
        limit: Option<usize>,
        discover: Option<bool>,
    },
    GetPost {
        post_id: String,
    },
    Profile {
        username: String,
    },
    SearchUsers {
        session: String,
        query: String,
        limit: Option<usize>,
    },
    Suggestions {
        session: String,
        limit: Option<usize>,
    },
    Followers {
        session: String,
        username: String,
        limit: Option<usize>,
    },
    Following {
        session: String,
        username: String,
        limit: Option<usize>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Response {
    Ok { message: String },
    LoginOk { session: String, expires_ms: u64 },
    Post { post: PostView },
    Feed { posts: Vec<PostView> },
    Profile { profile: ProfileView },
    Users { users: Vec<UserSummary> },
    Error { message: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct User {
    username: String,
    password_hash: String,
    salt: String,
    created_at: u64,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_blob_id: Option<String>,
    banner_blob_id: Option<String>,
    followers: Vec<String>,
    following: Vec<String>,
    posts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Session {
    token: String,
    username: String,
    expires_ms: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Comment {
    id: String,
    author: String,
    text: String,
    created_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Attachment {
    #[serde(rename = "blob_id_hex", alias = "blob_id")]
    blob_id: String,
    size: u64,
    #[serde(rename = "hash_hex", alias = "content_hash")]
    content_hash: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Post {
    id: String,
    author: String,
    text: String,
    attachments: Vec<Attachment>,
    created_at: u64,
    likes: Vec<String>,
    comments: Vec<Comment>,
}

#[derive(Debug, Serialize, Clone)]
struct PostView {
    id: String,
    author: String,
    text: String,
    attachments: Vec<Attachment>,
    created_at: u64,
    likes: usize,
    comments: Vec<CommentView>,
}

#[derive(Debug, Serialize, Clone)]
struct CommentView {
    id: String,
    author: String,
    text: String,
    created_at: u64,
}

#[derive(Debug, Serialize, Clone)]
struct ProfileView {
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_blob_id: Option<String>,
    banner_blob_id: Option<String>,
    followers: usize,
    following: usize,
    follower_list: Vec<String>,
    following_list: Vec<String>,
    posts: Vec<String>,
    created_at: u64,
}

#[derive(Debug, Serialize, Clone)]
struct UserSummary {
    username: String,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_blob_id: Option<String>,
    banner_blob_id: Option<String>,
    followers: usize,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let response = match serde_json::from_slice::<Request>(input) {
        Ok(req) => handle_request(req),
        Err(e) => Response::Error {
            message: format!("invalid json: {e}"),
        },
    };

    let encoded = serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"status\":\"error\",\"message\":\"serialize\"}".to_vec());
    let buf = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(32 * 1024)));
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

fn handle_request(req: Request) -> Response {
    match req {
        Request::Register { username, password } => match register(&username, &password) {
            Ok(_) => Response::Ok {
                message: "registered".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::UpdateProfile {
            session,
            display_name,
            bio,
            avatar_blob_id,
            banner_blob_id,
        } => match update_profile(&session, display_name, bio, avatar_blob_id, banner_blob_id) {
            Ok(_) => Response::Ok {
                message: "profile_updated".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::Login { username, password } => match login(&username, &password) {
            Ok(sess) => Response::LoginOk {
                session: sess.token,
                expires_ms: sess.expires_ms,
            },
            Err(e) => Response::Error { message: e },
        },
        Request::Logout { session } => match logout(&session) {
            Ok(_) => Response::Ok {
                message: "logged_out".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::CreatePost {
            session,
            text,
            attachments,
        } => match create_post(&session, &text, attachments) {
            Ok(p) => Response::Post { post: p },
            Err(e) => Response::Error { message: e },
        },
        Request::Follow { session, target } => match follow(&session, &target) {
            Ok(_) => Response::Ok {
                message: "following".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::Unfollow { session, target } => match unfollow(&session, &target) {
            Ok(_) => Response::Ok {
                message: "unfollowed".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::DeletePost { session, post_id } => match delete_post(&session, &post_id) {
            Ok(_) => Response::Ok {
                message: "deleted".into(),
            },
            Err(e) => Response::Error { message: e },
        },
        Request::Like { session, post_id } => match like(&session, &post_id) {
            Ok(p) => Response::Post { post: p },
            Err(e) => Response::Error { message: e },
        },
        Request::Comment {
            session,
            post_id,
            text,
        } => match comment(&session, &post_id, &text) {
            Ok(p) => Response::Post { post: p },
            Err(e) => Response::Error { message: e },
        },
        Request::Feed {
            session,
            limit,
            discover,
        } => match feed_internal(&session, limit.unwrap_or(20), discover.unwrap_or(false)) {
            Ok(posts) => Response::Feed { posts },
            Err(e) => Response::Error { message: e },
        },
        Request::GetPost { post_id } => match load_post_view(&post_id) {
            Ok(p) => Response::Post { post: p },
            Err(e) => Response::Error { message: e },
        },
        Request::Profile { username } => match profile(&username) {
            Ok(p) => Response::Profile { profile: p },
            Err(e) => Response::Error { message: e },
        },
        Request::SearchUsers {
            session,
            query,
            limit,
        } => match search_users(&session, &query, limit.unwrap_or(10)) {
            Ok(users) => Response::Users { users },
            Err(e) => Response::Error { message: e },
        },
        Request::Suggestions { session, limit } => match suggest_users(&session, limit.unwrap_or(5)) {
            Ok(users) => Response::Users { users },
            Err(e) => Response::Error { message: e },
        },
        Request::Followers {
            session,
            username,
            limit,
        } => match list_followers(&session, &username, limit.unwrap_or(50)) {
            Ok(users) => Response::Users { users },
            Err(e) => Response::Error { message: e },
        },
        Request::Following {
            session,
            username,
            limit,
        } => match list_following(&session, &username, limit.unwrap_or(50)) {
            Ok(users) => Response::Users { users },
            Err(e) => Response::Error { message: e },
        },
    }
}

fn register(username: &str, password: &str) -> Result<(), String> {
    if username.trim().is_empty() || password.len() < 6 {
        return Err("invalid username or password".into());
    }
    let user_key = user_key(username);
    if state_exists(&user_key)? {
        return Err("user already exists".into());
    }
    let salt = random_hex(16)?;
    let hash = hash_password(password, &salt);
    let now = now_ms();
    let user = User {
        username: username.to_string(),
        password_hash: hash,
        salt,
        created_at: now,
        display_name: None,
        bio: None,
        avatar_blob_id: None,
        banner_blob_id: None,
        followers: Vec::new(),
        following: Vec::new(),
        posts: Vec::new(),
    };
    store_json(&user_key, &user)?;
    append_user_index(username)
}

fn login(username: &str, password: &str) -> Result<Session, String> {
    let user_key = user_key(username);
    let user: User = load_json(&user_key)?.ok_or("user not found")?;
    let expected = hash_password(password, &user.salt);
    if expected != user.password_hash {
        return Err("invalid credentials".into());
    }
    let token = random_hex(24)?;
    let expires_ms = now_ms() + 86_400_000; // 24h
    let session = Session {
        token: token.clone(),
        username: username.to_string(),
        expires_ms,
    };
    store_json(&session_key(&token), &session)?;
    Ok(session)
}

fn logout(session_token: &str) -> Result<(), String> {
    delete_key(&session_key(session_token))
}

fn update_profile(
    session_token: &str,
    display_name: Option<String>,
    bio: Option<String>,
    avatar_blob_id: Option<String>,
    banner_blob_id: Option<String>,
) -> Result<(), String> {
    let mut user = require_session_user(session_token)?;

    if let Some(ref name) = display_name {
        if name.len() > 64 {
            return Err("display name too long".into());
        }
    }
    if let Some(ref b) = bio {
        if b.len() > 280 {
            return Err("bio too long".into());
        }
    }
    let avatar = resolve_profile_blob_id(avatar_blob_id)?;
    let banner = resolve_profile_blob_id(banner_blob_id)?;

    user.display_name = display_name;
    user.bio = bio;
    user.avatar_blob_id = avatar;
    user.banner_blob_id = banner;

    store_json(&user_key(&user.username), &user)
}

fn create_post(
    session_token: &str,
    text: &str,
    attachments: Vec<String>,
) -> Result<PostView, String> {
    let user = require_session_user(session_token)?;
    let post_id = format!("{}-{}", user.username, random_hex(12)?);
    let now = now_ms();
    if text.len() > 2048 {
        return Err("text too long".into());
    }
    let post = Post {
        id: post_id.clone(),
        author: user.username.clone(),
        text: text.to_string(),
        attachments: build_attachments(attachments)?,
        created_at: now,
        likes: Vec::new(),
        comments: Vec::new(),
    };
    store_json(&post_key(&post_id), &post)?;
    let mut user_mut = user.clone();
    user_mut.posts.push(post_id.clone());
    store_json(&user_key(&user_mut.username), &user_mut)?;
    append_post_index(&post_id)?;
    load_post_view(&post_id)
}

fn follow(session_token: &str, target: &str) -> Result<(), String> {
    let user = require_session_user(session_token)?;
    if user.username == target {
        return Err("cannot follow self".into());
    }
    let mut me = user.clone();
    if me.following.contains(&target.to_string()) {
        return Ok(());
    }
    me.following.push(target.to_string());
    store_json(&user_key(&me.username), &me)?;

    if let Some(mut other) = load_json::<User>(&user_key(target))? {
        if !other.followers.contains(&me.username) {
            other.followers.push(me.username.clone());
            store_json(&user_key(target), &other)?;
        }
    }
    Ok(())
}

fn unfollow(session_token: &str, target: &str) -> Result<(), String> {
    let user = require_session_user(session_token)?;
    let mut me = user.clone();
    me.following.retain(|u| u != target);
    store_json(&user_key(&me.username), &me)?;

    if let Some(mut other) = load_json::<User>(&user_key(target))? {
        other.followers.retain(|u| u != &me.username);
        store_json(&user_key(target), &other)?;
    }
    Ok(())
}

fn like(session_token: &str, post_id: &str) -> Result<PostView, String> {
    let user = require_session_user(session_token)?;
    let mut post: Post = load_json(&post_key(post_id))?.ok_or("post not found")?;
    if let Some(pos) = post.likes.iter().position(|u| u == &user.username) {
        post.likes.swap_remove(pos);
    } else {
        post.likes.push(user.username.clone());
    }
    store_json(&post_key(post_id), &post)?;
    load_post_view(post_id)
}

fn delete_post(session_token: &str, post_id: &str) -> Result<(), String> {
    let user = require_session_user(session_token)?;
    let mut post: Post = load_json(&post_key(post_id))?.ok_or("post not found")?;
    if post.author != user.username {
        return Err("forbidden".into());
    }
    // Remove from author list
    let mut author = user;
    author.posts.retain(|p| p != post_id);
    store_json(&user_key(&author.username), &author)?;
    // Remove post record
    delete_key(&post_key(post_id))?;
    // Remove from index
    remove_post_index(post_id)?;
    Ok(())
}

fn comment(session_token: &str, post_id: &str, text: &str) -> Result<PostView, String> {
    if text.is_empty() || text.len() > 1024 {
        return Err("invalid comment length".into());
    }
    let user = require_session_user(session_token)?;
    let mut post: Post = load_json(&post_key(post_id))?.ok_or("post not found")?;
    let comment = Comment {
        id: format!("{}-{}", post_id, random_hex(8)?),
        author: user.username,
        text: text.to_string(),
        created_at: now_ms(),
    };
    post.comments.push(comment);
    store_json(&post_key(post_id), &post)?;
    load_post_view(post_id)
}

fn feed_internal(session_token: &str, limit: usize, discover: bool) -> Result<Vec<PostView>, String> {
    let user = require_session_user(session_token)?;
    let mut ids = load_post_index()?;
    if !discover {
        ids.retain(|id| {
            if let Ok(post) = load_post_author(id) {
                post == user.username || user.following.contains(&post)
            } else {
                false
            }
        });
    }
    ids.truncate(limit.min(200));
    let now = now_ms();
    let mut scored: Vec<(f64, PostView)> = Vec::new();
    for id in ids {
        if let Ok(p) = load_post_view(&id) {
            let score = post_score(&p, now);
            scored.push((score, p));
        }
    }
    for (s, _) in scored.iter_mut() {
        if let Ok(rand) = random_u64() {
            let jitter = (rand % 100) as f64 / 10_000.0;
            *s += jitter;
        }
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Vec::new();
    for (_, p) in scored.into_iter().take(limit.min(100)) {
        out.push(p);
    }
    Ok(out)
}

fn profile(username: &str) -> Result<ProfileView, String> {
    let user: User = load_json(&user_key(username))?.ok_or("user not found")?;
    let avatar_blob_id = normalize_blob_id_opt(user.avatar_blob_id)?;
    let banner_blob_id = normalize_blob_id_opt(user.banner_blob_id)?;
    Ok(ProfileView {
        username: user.username,
        display_name: user.display_name,
        bio: user.bio,
        avatar_blob_id,
        banner_blob_id,
        followers: user.followers.len(),
        following: user.following.len(),
        follower_list: user.followers,
        following_list: user.following,
        posts: user.posts,
        created_at: user.created_at,
    })
}

fn load_user_summary(username: &str) -> Result<Option<UserSummary>, String> {
    if let Some(user) = load_json::<User>(&user_key(username))? {
        let avatar_blob_id = normalize_blob_id_opt(user.avatar_blob_id)?;
        let banner_blob_id = normalize_blob_id_opt(user.banner_blob_id)?;
        return Ok(Some(UserSummary {
            username: user.username,
            display_name: user.display_name,
            bio: user.bio,
            avatar_blob_id,
            banner_blob_id,
            followers: user.followers.len(),
        }));
    }
    Ok(None)
}

fn search_users(session_token: &str, query: &str, limit: usize) -> Result<Vec<UserSummary>, String> {
    // Ensure session is valid even if result doesn't depend on user
    let _ = require_session_user(session_token)?;
    let idx = load_user_index()?;
    let q = query.to_lowercase();
    let mut results = Vec::new();
    for username in idx {
        if results.len() >= limit {
            break;
        }
        if username.to_lowercase().contains(&q) {
            if let Some(summary) = load_user_summary(&username)? {
                results.push(summary);
            }
        }
    }
    Ok(results)
}

fn suggest_users(session_token: &str, limit: usize) -> Result<Vec<UserSummary>, String> {
    let me = require_session_user(session_token)?;
    let idx = load_user_index()?;
    let mut candidates: Vec<UserSummary> = Vec::new();
    for username in idx {
        if username == me.username || me.following.contains(&username) {
            continue;
        }
        if let Some(summary) = load_user_summary(&username)? {
            candidates.push(summary);
        }
    }
    // Shuffle using random bytes for basic entropy
    for i in 0..candidates.len() {
        if let Ok(rand) = random_u64() {
            let j = (rand as usize) % candidates.len();
            candidates.swap(i, j);
        }
    }
    candidates.truncate(limit.min(20));
    Ok(candidates)
}

fn list_followers(session_token: &str, username: &str, limit: usize) -> Result<Vec<UserSummary>, String> {
    let _ = require_session_user(session_token)?;
    let user: User = load_json(&user_key(username))?.ok_or("user not found")?;
    let mut out = Vec::new();
    for name in user.followers.iter().take(limit) {
        if let Some(summary) = load_user_summary(name)? {
            out.push(summary);
        }
    }
    Ok(out)
}

fn list_following(session_token: &str, username: &str, limit: usize) -> Result<Vec<UserSummary>, String> {
    let _ = require_session_user(session_token)?;
    let user: User = load_json(&user_key(username))?.ok_or("user not found")?;
    let mut out = Vec::new();
    for name in user.following.iter().take(limit) {
        if let Some(summary) = load_user_summary(name)? {
            out.push(summary);
        }
    }
    Ok(out)
}

fn load_post_view(post_id: &str) -> Result<PostView, String> {
    let mut post: Post = load_json(&post_key(post_id))?.ok_or("post not found")?;
    normalize_attachments(&mut post.attachments)?;
    let comments: Vec<CommentView> = post
        .comments
        .iter()
        .map(|c| CommentView {
            id: c.id.clone(),
            author: c.author.clone(),
            text: c.text.clone(),
            created_at: c.created_at,
        })
        .collect();
    Ok(PostView {
        id: post.id,
        author: post.author,
        text: post.text,
        attachments: post.attachments,
        created_at: post.created_at,
        likes: post.likes.len(),
        comments,
    })
}

fn load_post_author(post_id: &str) -> Result<String, String> {
    let post: Post = load_json(&post_key(post_id))?.ok_or("post not found")?;
    Ok(post.author)
}

fn post_score(p: &PostView, now: u64) -> f64 {
    let age_hours = ((now.saturating_sub(p.created_at)) as f64) / 3_600_000.0;
    let recency = 1.0 / (1.0 + age_hours);
    let likes = p.likes as f64;
    recency * 2.0 + likes.sqrt()
}

fn require_session_user(session_token: &str) -> Result<User, String> {
    let session: Session = load_json(&session_key(session_token))?.ok_or("invalid session")?;
    if session.expires_ms < now_ms() {
        delete_key(&session_key(session_token))?;
        return Err("session expired".into());
    }
    load_json(&user_key(&session.username))?.ok_or("user not found".into())
}

// State helpers
fn store_json<T: Serialize>(key: &str, val: &T) -> Result<(), String> {
    let data = serde_json::to_vec(val).map_err(|e| e.to_string())?;
    state_put(key.as_bytes(), &data)
}

fn load_json<T: for<'de> Deserialize<'de>>(key: &str) -> Result<Option<T>, String> {
    match state_get(key.as_bytes())? {
        Some(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
        None => Ok(None),
    }
}

fn delete_key(key: &str) -> Result<(), String> {
    state_put(key.as_bytes(), &[])?;
    Ok(())
}

fn state_exists(key: &str) -> Result<bool, String> {
    Ok(state_get(key.as_bytes())?.is_some())
}

fn append_post_index(post_id: &str) -> Result<(), String> {
    let mut idx = load_post_index()?;
    idx.push(post_id.to_string());
    idx.sort();
    store_json(POST_INDEX_KEY, &idx)
}

fn load_post_index() -> Result<Vec<String>, String> {
    load_json(POST_INDEX_KEY).map(|opt| opt.unwrap_or_default())
}

fn remove_post_index(post_id: &str) -> Result<(), String> {
    let mut idx = load_post_index()?;
    idx.retain(|id| id != post_id);
    store_json(POST_INDEX_KEY, &idx)
}

fn append_user_index(username: &str) -> Result<(), String> {
    let mut idx = load_user_index()?;
    if !idx.contains(&username.to_string()) {
        idx.push(username.to_string());
        idx.sort();
        store_json(USER_INDEX_KEY, &idx)?;
    }
    Ok(())
}

fn load_user_index() -> Result<Vec<String>, String> {
    load_json(USER_INDEX_KEY).map(|opt| opt.unwrap_or_default())
}

// Util helpers
fn user_key(username: &str) -> String {
    format!("user:{username}")
}

fn session_key(token: &str) -> String {
    format!("session:{token}")
}

fn post_key(id: &str) -> String {
    format!("post:{id}")
}

fn hash_password(password: &str, salt: &str) -> String {
    let mut h = Hasher::new();
    h.update(password.as_bytes());
    h.update(salt.as_bytes());
    hex::encode(h.finalize().as_bytes())
}

fn random_hex(len: usize) -> Result<String, String> {
    let mut buf = vec![0u8; len];
    let rc = unsafe { onvm_random_bytes(buf.as_mut_ptr() as i32, len as i32) };
    if rc != 0 {
        return Err(format!("random failed: {rc}"));
    }
    Ok(hex::encode(buf))
}

fn now_ms() -> u64 {
    let ts = unsafe { onvm_now_ms() };
    if ts < 0 {
        0
    } else {
        ts as u64
    }
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

fn state_get(key: &[u8]) -> Result<Option<Vec<u8>>, String> {
    let mut cap = 512;
    for _ in 0..6 {
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
            return Ok(None);
        }
        if len > 0 {
            buf.truncate(len as usize);
            return Ok(Some(buf));
        }
        cap = (-len) as usize;
    }
    Err("state get failed".into())
}

fn random_u64() -> Result<u64, String> {
    let mut buf = [0u8; 8];
    let rc = unsafe { onvm_random_bytes(buf.as_mut_ptr() as i32, buf.len() as i32) };
    if rc != 0 {
        return Err(format!("random failed: {rc}"));
    }
    Ok(u64::from_le_bytes(buf))
}

#[derive(Debug, Clone)]
struct BlobId {
    raw: [u8; 32],
}

impl BlobId {
    fn parse(id: &str) -> Result<Self, String> {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            return Err("empty blob id".into());
        }
        let mut raw = [0u8; 32];
        let rc = unsafe {
            onvm_blob_addr(
                trimmed.as_ptr() as i32,
                trimmed.len() as i32,
                raw.as_mut_ptr() as i32,
            )
        };
        if rc != 0 {
            return Err(format!("invalid blob id: {rc}"));
        }
        Ok(Self { raw })
    }

    fn require_exists(id: &str) -> Result<Self, String> {
        let trimmed = id.trim();
        let blob = Self::parse(trimmed)?;
        match blob.exists()? {
            true => Ok(blob),
            false => Err(format!("blob {trimmed} missing")),
        }
    }

    fn canonical(&self) -> String {
        format!("{}{}", BLOB_PREFIX, hex::encode(self.raw))
    }

    fn exists(&self) -> Result<bool, String> {
        let rc = unsafe { onvm_blob_exists(self.raw.as_ptr() as i32, self.raw.len() as i32) };
        match rc {
            1 => Ok(true),
            0 => Ok(false),
            code => Err(format!("blob exists failed: {code}")),
        }
    }

    fn len(&self) -> Result<u64, String> {
        let len = unsafe { onvm_blob_len(self.raw.as_ptr() as i32, self.raw.len() as i32) };
        if len < 0 {
            return Err(format!("failed to read blob length: {len}"));
        }
        Ok(len as u64)
    }

    fn content_hash(&self) -> Result<[u8; 32], String> {
        let mut hash = [0u8; 32];
        let rc = unsafe {
            onvm_blob_hash(
                self.raw.as_ptr() as i32,
                self.raw.len() as i32,
                hash.as_mut_ptr() as i32,
            )
        };
        if rc != 0 {
            return Err(format!("failed to read blob hash: {rc}"));
        }
        Ok(hash)
    }
}

fn normalize_blob_id(id: &str) -> Result<String, String> {
    Ok(BlobId::parse(id)?.canonical())
}

fn normalize_blob_id_opt(id: Option<String>) -> Result<Option<String>, String> {
    match id {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(normalize_blob_id(trimmed)?))
            }
        }
        None => Ok(None),
    }
}

fn resolve_profile_blob_id(id: Option<String>) -> Result<Option<String>, String> {
    match id {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(BlobId::require_exists(trimmed)?.canonical()))
            }
        }
        None => Ok(None),
    }
}

fn normalize_attachments(attachments: &mut [Attachment]) -> Result<(), String> {
    for attachment in attachments.iter_mut() {
        attachment.blob_id = normalize_blob_id(&attachment.blob_id)?;
    }
    Ok(())
}

fn build_attachments(ids: Vec<String>) -> Result<Vec<Attachment>, String> {
    let mut out = Vec::new();
    for id in ids {
        let blob = BlobId::require_exists(&id)?;
        let size = blob.len()?;
        let hash = blob.content_hash()?;
        out.push(Attachment {
            blob_id: blob.canonical(),
            size,
            content_hash: hex::encode(hash),
        });
    }
    Ok(out)
}

// Host functions
extern "C" {
    fn onvm_state_put(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
    fn onvm_state_get(key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32) -> i32;
    fn onvm_random_bytes(out_ptr: i32, out_len: i32) -> i32;
    fn onvm_now_ms() -> i64;
    fn onvm_blob_addr(id_ptr: i32, id_len: i32, out_ptr: i32) -> i32;
    fn onvm_blob_exists(id_ptr: i32, id_len: i32) -> i32;
    fn onvm_blob_len(id_ptr: i32, id_len: i32) -> i64;
    fn onvm_blob_hash(id_ptr: i32, id_len: i32, out_ptr: i32) -> i32;
}

const BLOB_PREFIX: &str = "blob";
const POST_INDEX_KEY: &str = "__posts";
const USER_INDEX_KEY: &str = "__users";
