//! Integration tests for the auth-service (register, login, refresh, logout, /users/me, devices).

mod common;

use common::{login_user, register_user, start_auth_server, unique_suffix, TestDb};

// ---------------------------------------------------------------------------
// Auth: Register
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_and_login_happy_path() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    // Register
    let (token, refresh, user_id) = register_user(
        &base,
        &format!("alice_{s}"),
        &format!("alice_{s}@test.io"),
        "password123",
    )
    .await;
    assert!(!token.is_empty());
    assert!(!refresh.is_empty());

    // Login with same credentials
    let (login_token, login_refresh) =
        login_user(&base, &format!("alice_{s}@test.io"), "password123").await;
    assert!(!login_token.is_empty());
    assert!(!login_refresh.is_empty());
    // Tokens from login should differ from register tokens
    assert_ne!(token, login_token);

    // GET /users/me with the login token
    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{base}/users/me"))
        .header("Authorization", format!("Bearer {login_token}"))
        .send()
        .await
        .expect("get me");
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("parse me");
    assert_eq!(body["id"].as_str().unwrap(), user_id.to_string());
    assert_eq!(body["username"].as_str().unwrap(), format!("alice_{s}"));
}

#[tokio::test]
async fn register_duplicate_username_fails() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    register_user(
        &base,
        &format!("dup_{s}"),
        &format!("dup1_{s}@test.io"),
        "password123",
    )
    .await;

    // Second registration with same username should fail
    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/register"))
        .json(&serde_json::json!({
            "username": format!("dup_{s}"),
            "email": format!("dup2_{s}@test.io"),
            "password": "password123",
            "display_name": "Dup",
        }))
        .send()
        .await
        .expect("register dup request");

    assert_eq!(
        res.status(),
        409,
        "expected conflict for duplicate username"
    );
}

#[tokio::test]
async fn register_duplicate_email_fails() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    register_user(
        &base,
        &format!("orig_{s}"),
        &format!("same_{s}@test.io"),
        "password123",
    )
    .await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/register"))
        .json(&serde_json::json!({
            "username": format!("other_{s}"),
            "email": format!("same_{s}@test.io"),
            "password": "password123",
            "display_name": "Other",
        }))
        .send()
        .await
        .expect("register dup email request");

    assert_eq!(res.status(), 409, "expected conflict for duplicate email");
}

#[tokio::test]
async fn login_wrong_password() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    register_user(
        &base,
        &format!("bob_{s}"),
        &format!("bob_{s}@test.io"),
        "correctpass",
    )
    .await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/login"))
        .json(&serde_json::json!({
            "email": format!("bob_{s}@test.io"),
            "password": "wrongpass",
        }))
        .send()
        .await
        .expect("login wrong pass");

    assert_eq!(res.status(), 401, "expected 401 for wrong password");
}

#[tokio::test]
async fn login_nonexistent_user() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/login"))
        .json(&serde_json::json!({
            "email": "nonexistent@test.io",
            "password": "whatever",
        }))
        .send()
        .await
        .expect("login nonexistent");

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// Auth: Token refresh
// ---------------------------------------------------------------------------

#[tokio::test]
async fn refresh_token_happy_path() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (_token, refresh, _uid) = register_user(
        &base,
        &format!("ref_{s}"),
        &format!("ref_{s}@test.io"),
        "password123",
    )
    .await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .expect("refresh request");

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.expect("parse refresh body");
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    // Old refresh token should be rotated
    assert_ne!(body["refresh_token"].as_str().unwrap(), refresh);
}

#[tokio::test]
async fn refresh_with_invalid_token_fails() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;

    let client = reqwest::Client::new();
    let res = client
        .post(format!("http://{base}/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": "bogus-token-value" }))
        .send()
        .await
        .expect("refresh invalid");

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// Auth: Logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn logout_revokes_refresh_token() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (token, refresh, _uid) = register_user(
        &base,
        &format!("out_{s}"),
        &format!("out_{s}@test.io"),
        "password123",
    )
    .await;

    let client = reqwest::Client::new();

    // Logout
    let res = client
        .post(format!("http://{base}/auth/logout"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .expect("logout request");

    assert_eq!(res.status(), 200);

    // Refresh with the revoked token should fail
    let res = client
        .post(format!("http://{base}/auth/refresh"))
        .json(&serde_json::json!({ "refresh_token": refresh }))
        .send()
        .await
        .expect("refresh after logout");

    assert_eq!(res.status(), 401, "refresh should fail after logout");
}

// ---------------------------------------------------------------------------
// Auth: GET /users/me (unauthorized)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_me_unauthorized() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{base}/users/me"))
        .send()
        .await
        .expect("get me no auth");

    assert_eq!(res.status(), 401);
}

// ---------------------------------------------------------------------------
// Auth: Device CRUD
// ---------------------------------------------------------------------------

#[tokio::test]
async fn device_crud() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;
    let s = unique_suffix();

    let (token, _refresh, _uid) = register_user(
        &base,
        &format!("dev_{s}"),
        &format!("dev_{s}@test.io"),
        "password123",
    )
    .await;

    let client = reqwest::Client::new();

    // Create device
    let res = client
        .post(format!("http://{base}/users/me/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({
            "name": "iPhone 15",
            "platform": "ios",
        }))
        .send()
        .await
        .expect("create device");

    assert_eq!(res.status(), 201);
    let device: serde_json::Value = res.json().await.expect("parse device body");
    let device_id = device["id"].as_str().expect("device id");

    // List devices
    let res = client
        .get(format!("http://{base}/users/me/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("list devices");

    assert_eq!(res.status(), 200);
    let devices: Vec<serde_json::Value> = res.json().await.expect("parse devices");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["name"].as_str().unwrap(), "iPhone 15");

    // Delete device
    let res = client
        .delete(format!("http://{base}/users/me/devices/{device_id}"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("delete device");

    assert_eq!(res.status(), 204);

    // List devices: should be empty
    let res = client
        .get(format!("http://{base}/users/me/devices"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .expect("list devices after delete");

    assert_eq!(res.status(), 200);
    let devices: Vec<serde_json::Value> = res.json().await.expect("parse devices empty");
    assert!(devices.is_empty());
}

// ---------------------------------------------------------------------------
// Auth: Validation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn register_validation_errors() {
    let db = TestDb::start().await;
    let base = start_auth_server(db.redis.clone()).await;

    let client = reqwest::Client::new();

    // Too short username
    let res = client
        .post(format!("http://{base}/auth/register"))
        .json(&serde_json::json!({
            "username": "ab",
            "email": "valid@test.io",
            "password": "password123",
            "display_name": "Valid",
        }))
        .send()
        .await
        .expect("register short username");

    assert_eq!(res.status(), 400, "short username should be rejected");

    // Too short password
    let res = client
        .post(format!("http://{base}/auth/register"))
        .json(&serde_json::json!({
            "username": "validuser",
            "email": "valid2@test.io",
            "password": "short",
            "display_name": "Valid",
        }))
        .send()
        .await
        .expect("register short password");

    assert_eq!(res.status(), 400, "short password should be rejected");

    // Invalid email
    let res = client
        .post(format!("http://{base}/auth/register"))
        .json(&serde_json::json!({
            "username": "validuser2",
            "email": "not-an-email",
            "password": "password123",
            "display_name": "Valid",
        }))
        .send()
        .await
        .expect("register bad email");

    assert_eq!(res.status(), 400, "bad email should be rejected");
}
