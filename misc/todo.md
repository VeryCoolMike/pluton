# pluton-server
- [x] Add file uploads
- [x] Make it so that when a client joins you receive message attachments
- [ ] Add VC
- [ ] Fix bug with user not being removed from peer map (or something) when being disconnected

## pluton-server-http
- [x] Add file uploads
- [ ] Add HTTPS support
- [ ] Fix bug with large file uploads causing 400

# pluton-core

# pluton-home

# pluton-cli
- [x] Add file uploads
- [x] Add file rendering
- [ ] Add file downloads
- [x] Fix sending file data twice (once when uploading once when sending message)

# bug fixes

## `unbounded_send().unwrap()` panics when a client disconnects (connections.rs:99, 201, 226)
**How it happens:** When broadcasting messages (join alerts, chat messages, or requested messages), the code calls `.unbounded_send().unwrap()` on the channel tx. If a client disconnects between when the peer_map is read and when the send happens, the channel is closed and `unwrap()` panics, crashing the entire connection handler.
**Recommended fix:** Replace `.unwrap()` with `let _ =` or log the error. A closed channel just means the client left — not worth crashing over.

## `msg.to_text().unwrap()` panics on non-text messages (connections.rs:146)
**How it happens:** The debug print calls `msg.to_text().unwrap()` before the match statement checks the message type. If a client sends a Binary, Ping, or Pong frame, `to_text()` returns Err and the unwrap panics.
**Recommended fix:** Use `msg.to_text().unwrap_or("<non-text>")` or move the print inside the `Message::Text` arm.

## `Nonce::from_slice` panics on wrong-length nonce (cryptography/mod.rs:77, 104)
**How it happens:** `from_base64()` returns an empty Vec on invalid input (bad base64 string). `Nonce::from_slice` expects exactly 12 bytes and panics if the length is wrong. A corrupted config file or tampered nonce string will crash the program.
**Recommended fix:** Check `nonce_bytes.len() == 12` before calling `from_slice`, return an error otherwise.

## `from_utf8().unwrap()` panics in `get_servers` (account_management/mod.rs:61, 64)
**How it happens:** `from_base64()` can return arbitrary bytes if the stored servers string is corrupted. `std::str::from_utf8` will panic on non-UTF-8 bytes. Also, `from_utf8` is called twice on the same data unnecessarily.
**Recommended fix:** Use `from_utf8()?.to_string()` with `?` instead of `.unwrap()`, and only call it once.

## `check_password_strength` has inverted special character check (account_management/mod.rs:251)
**How it happens:** The condition `password.chars().any(|c| !c.is_alphanumeric())` warns when the password DOES contain special characters (the `any` returns true if a non-alphanumeric char exists). The warning text says "without any special characters" but fires when they ARE present. Should be `!password.chars().any(...)` or `.all(|c| c.is_alphanumeric())`.
**Recommended fix:** Change to `if !password.chars().any(|c| !c.is_alphanumeric())` or `if password.chars().all(|c| c.is_alphanumeric())`.

## Role ID silently truncated from u64 to u8 (database.rs:255-257)
**How it happens:** Role IDs are stored as u64 in SQLite (INTEGER PRIMARY KEY auto-increments) but cast to u8 with `role_id as u8`. If a role has ID > 255, it wraps around silently (e.g., role 256 becomes role 0), causing wrong permission grants.
**Recommended fix:** Use `u8::try_from(role_id)?` or just use u64 for role IDs everywhere.

## `duration_since(UNIX_EPOCH).unwrap()` can panic (connections.rs:174)
**How it happens:** If the system clock is set before 1970 (e.g., NTP correction on boot, VM with bad clock), `duration_since(UNIX_EPOCH)` returns Err and the unwrap panics.
**Recommended fix:** Use `.map_err(|_| ...)? ` or `.unwrap_or_default()`.

## Integer overflow in timestamp check (pluton-home/main.rs:101)
**How it happens:** `(current_time - request.timestamp).abs()` — if an attacker sends `timestamp = i64::MIN` and current_time is positive, the subtraction overflows in debug mode (panic) or wraps in release mode (passes the check when it shouldn't). Also `.abs()` on `i64::MIN` itself is UB in release.
**Recommended fix:** Use `current_time.saturating_sub(request.timestamp).unsigned_abs() > 60` or check each bound separately.

## `from_base64url` replaces "." with "=" (base64.rs:109)
**How it happens:** Base64url does not use "." as a padding character — it uses no padding at all. If a base64url string legitimately contains data that decodes from characters near ".", this replacement corrupts the input. The padding restoration on lines 117-119 already handles missing padding correctly, making the dot replacement both wrong and redundant.
**Recommended fix:** Remove line 109 (`new_input = new_input.replace(".", "=");`).

## Server password stored in plaintext in database (creation.rs:137)
**How it happens:** The server password is inserted directly into the SQLite database as plaintext. Anyone with read access to `server_data.db` can see the password.
**Recommended fix:** Hash the password with Argon2 (already used elsewhere in the project) before storing it.

## No duplicate profile check in `create_profile` (pluton-home/main.rs:193-198)
**How it happens:** The `create_profile` endpoint inserts a new user without checking if one already exists with that verifying key. If a user calls it twice, they'll get a database error (if UNIQUE constraint exists) or duplicate entries (if not — and there's no UNIQUE constraint on verifying_key in the CREATE TABLE).
**Recommended fix:** Add a UNIQUE constraint on `verifying_key` in the users table, or check for existence before inserting and return an appropriate status code.

## `expect()` on websocket accept crashes server for all clients (connections.rs:24)
**How it happens:** `tokio_tungstenite::accept_async(raw_stream).await.expect(...)` — if a client sends garbage (not a valid HTTP upgrade request), this panics and kills the task. While it won't crash the whole server (it's in a spawned task), it's still an unclean error path.
**Recommended fix:** Use `?` or match on the error and return early with a log message.

