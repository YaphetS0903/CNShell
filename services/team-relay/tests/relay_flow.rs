use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use cnshell_team_relay::{
    RelayStore,
    models::{
        AccountSessionOutput, ControlLeaseOutput, DeviceChallengeOutput, DeviceRegistration,
        DeviceSessionOutput, RelayAuditEvent, RoomView, RoutedRoomInvitation,
        TeamTerminalEncryptedFrame, TeamTerminalInvitation, WorkspaceInvitationOutput,
        WorkspaceSnapshot,
    },
    router,
};
use ed25519_dalek::{Signer, SigningKey};
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use tokio::task::JoinHandle;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

struct TestServer {
    base_url: String,
    ws_url: String,
    _directory: TempDir,
    handle: JoinHandle<()>,
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

struct TestDevice {
    registration: DeviceRegistration,
    signing: SigningKey,
}

async fn start_server() -> TestServer {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("relay.sqlite");
    let database_url = format!("sqlite://{}?mode=rwc", database.display());
    let store = RelayStore::open(&database_url).await.unwrap();
    let app = router(store);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    TestServer {
        base_url: format!("http://{address}"),
        ws_url: format!("ws://{address}"),
        _directory: directory,
        handle,
    }
}

fn device(seed: u8) -> TestDevice {
    let signing = SigningKey::from_bytes(&[seed; 32]);
    let signing_public = signing.verifying_key().to_bytes();
    let encryption_public = [seed.wrapping_add(1); 32];
    let mut digest = Sha256::new();
    digest.update(b"cnshell-team-device-v1\0");
    digest.update(encryption_public);
    digest.update(signing_public);
    TestDevice {
        registration: DeviceRegistration {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("Device {seed}"),
            encryption_public_key: format!("x25519:{}", URL_SAFE_NO_PAD.encode(encryption_public)),
            signing_public_key: format!("ed25519:{}", URL_SAFE_NO_PAD.encode(signing_public)),
            fingerprint: format!("sha256:{:x}", digest.finalize()),
        },
        signing,
    }
}

async fn register(client: &Client, server: &TestServer, email: &str) -> AccountSessionOutput {
    client
        .post(format!("{}/v1/accounts/register", server.base_url))
        .json(&json!({
            "email": email,
            "password": "correct horse battery staple",
            "displayName": email.split('@').next().unwrap()
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

fn sign_canonical<T: Serialize>(value: &T, key: &SigningKey) -> String {
    let payload = serde_jcs::to_vec(value).unwrap();
    format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(key.sign(&payload).to_bytes())
    )
}

fn signed_invitation(
    room: &RoomView,
    recipient: &DeviceSessionOutput,
    host: &TestDevice,
) -> TeamTerminalInvitation {
    let created = Utc::now();
    let mut invitation = TeamTerminalInvitation {
        schema_version: 1,
        room_id: room.id.clone(),
        workspace_id: room.workspace_id.clone(),
        key_epoch: room.key_epoch,
        host_member_id: room.host_member_id.clone(),
        host_device_id: room.host_device_id.clone(),
        recipient_member_id: recipient.member_id.clone(),
        recipient_device_id: recipient.device_id.clone(),
        ephemeral_public_key: format!("x25519:{}", URL_SAFE_NO_PAD.encode([7_u8; 32])),
        key_nonce: URL_SAFE_NO_PAD.encode([8_u8; 12]),
        wrapped_room_key: URL_SAFE_NO_PAD.encode([9_u8; 48]),
        replay_from_sequence: 0,
        next_input_sequence: 1,
        created_at: created.to_rfc3339(),
        expires_at: (created + ChronoDuration::minutes(5)).to_rfc3339(),
        signature: None,
    };
    invitation.signature = Some(sign_canonical(&invitation, &host.signing));
    invitation
}

fn signed_frame(
    room: &RoomView,
    sender: &DeviceSessionOutput,
    signing: &SigningKey,
    direction: &str,
    sequence: u64,
    lease: Option<&ControlLeaseOutput>,
    marker: u8,
) -> TeamTerminalEncryptedFrame {
    let mut frame = TeamTerminalEncryptedFrame {
        schema_version: 1,
        workspace_id: room.workspace_id.clone(),
        room_id: room.id.clone(),
        key_epoch: room.key_epoch,
        sender_member_id: sender.member_id.clone(),
        sender_device_id: sender.device_id.clone(),
        direction: direction.into(),
        kind: "terminal".into(),
        sequence,
        lease_id: lease.map(|value| value.lease_id.clone()),
        lease_generation: lease.map_or(0, |value| value.generation),
        nonce: URL_SAFE_NO_PAD.encode([marker; 12]),
        ciphertext: URL_SAFE_NO_PAD.encode([marker; 32]),
        signature: None,
    };
    frame.signature = Some(sign_canonical(&frame, signing));
    frame
}

async fn connect_socket(
    server: &TestServer,
    room_id: &str,
    token: &str,
    after_sequence: u64,
) -> WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>> {
    let mut request = format!(
        "{}/v1/terminal/ws/{room_id}?afterSequence={after_sequence}",
        server.ws_url
    )
    .into_client_request()
    .unwrap();
    request
        .headers_mut()
        .insert("Authorization", bearer(token).parse().unwrap());
    connect_async(request).await.unwrap().0
}

async fn receive_json(
    socket: &mut WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
) -> Value {
    let message = tokio::time::timeout(std::time::Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let Message::Text(text) = message else {
        panic!("expected text WebSocket message")
    };
    serde_json::from_str(&text).unwrap()
}

#[tokio::test]
async fn accounts_rbac_encrypted_websocket_replay_and_revocation_round_trip() {
    let server = start_server().await;
    let client = Client::new();
    let host_device = device(11);
    let recipient_device = device(22);
    let alice = register(&client, &server, "alice@example.com").await;
    let bob = register(&client, &server, "bob@example.com").await;
    let workspace_id = uuid::Uuid::new_v4().to_string();
    let host_member_id = uuid::Uuid::new_v4().to_string();
    let host_session: DeviceSessionOutput = client
        .post(format!("{}/v1/workspaces/bootstrap", server.base_url))
        .header("Authorization", bearer(&alice.token))
        .json(&json!({
            "workspaceId": workspace_id,
            "workspaceName": "Example Team",
            "memberId": host_member_id,
            "device": host_device.registration
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();

    let recipient_member_id = uuid::Uuid::new_v4().to_string();
    let invitation: WorkspaceInvitationOutput = client
        .post(format!(
            "{}/v1/workspaces/{}/invitations",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .json(&json!({
            "email": "bob@example.com",
            "role": "operator",
            "memberId": recipient_member_id
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let recipient_session: DeviceSessionOutput = client
        .post(format!("{}/v1/invitations/accept", server.base_url))
        .header("Authorization", bearer(&bob.token))
        .json(&json!({
            "token": invitation.token,
            "device": recipient_device.registration
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recipient_session.key_epoch, 2);

    let snapshot: WorkspaceSnapshot = client
        .get(format!(
            "{}/v1/workspaces/{}",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(snapshot.key_epoch, 2);
    assert_eq!(snapshot.members.len(), 2);
    assert_eq!(snapshot.devices.len(), 2);

    let denied = client
        .post(format!(
            "{}/v1/workspaces/{}/invitations",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&recipient_session.token))
        .json(&json!({
            "email": "third@example.com",
            "role": "viewer",
            "memberId": uuid::Uuid::new_v4().to_string()
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let challenge: DeviceChallengeOutput = client
        .post(format!("{}/v1/device-challenges", server.base_url))
        .json(&json!({
            "workspaceId": workspace_id,
            "deviceId": recipient_session.device_id
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let challenge_payload = format!(
        "cnshell-relay-device-session-v1\0{}\0{}",
        challenge.challenge_id, challenge.challenge
    );
    let challenge_signature = format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(
            recipient_device
                .signing
                .sign(challenge_payload.as_bytes())
                .to_bytes()
        )
    );
    let refreshed_session: DeviceSessionOutput = client
        .post(format!("{}/v1/device-sessions", server.base_url))
        .json(&json!({
            "challengeId": challenge.challenge_id,
            "challenge": challenge.challenge,
            "signature": challenge_signature
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let replayed_challenge = client
        .post(format!("{}/v1/device-sessions", server.base_url))
        .json(&json!({
            "challengeId": challenge.challenge_id,
            "challenge": challenge.challenge,
            "signature": challenge_signature
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(replayed_challenge.status(), StatusCode::UNAUTHORIZED);

    let room_id = uuid::Uuid::new_v4().to_string();
    let room: RoomView = client
        .post(format!("{}/v1/terminal/rooms", server.base_url))
        .header("Authorization", bearer(&host_session.token))
        .json(&json!({ "roomId": room_id, "keyEpoch": 2 }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let room_invitation = signed_invitation(&room, &recipient_session, &host_device);
    client
        .post(format!(
            "{}/v1/terminal/rooms/{}/invitation",
            server.base_url, room.id
        ))
        .header("Authorization", bearer(&host_session.token))
        .json(&json!({ "invitation": room_invitation }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let routed: Vec<RoutedRoomInvitation> = client
        .get(format!("{}/v1/terminal/invitations", server.base_url))
        .header("Authorization", bearer(&refreshed_session.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(routed.len(), 1);
    assert_eq!(routed[0].room_id, room.id);
    client
        .post(format!(
            "{}/v1/terminal/rooms/{}/join",
            server.base_url, room.id
        ))
        .header("Authorization", bearer(&refreshed_session.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    let mut host_socket = connect_socket(&server, &room.id, &host_session.token, 0).await;
    let mut recipient_socket = connect_socket(&server, &room.id, &refreshed_session.token, 0).await;
    let first_output = signed_frame(
        &room,
        &host_session,
        &host_device.signing,
        "output",
        1,
        None,
        31,
    );
    host_socket
        .send(Message::Text(
            json!({ "type": "frame", "frame": first_output })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        receive_json(&mut recipient_socket).await["frame"]["sequence"],
        1
    );
    assert_eq!(receive_json(&mut host_socket).await["frame"]["sequence"], 1);

    recipient_socket.close(None).await.unwrap();
    let second_output = signed_frame(
        &room,
        &host_session,
        &host_device.signing,
        "output",
        2,
        None,
        32,
    );
    host_socket
        .send(Message::Text(
            json!({ "type": "frame", "frame": second_output })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(receive_json(&mut host_socket).await["frame"]["sequence"], 2);
    let mut recipient_socket = connect_socket(&server, &room.id, &refreshed_session.token, 1).await;
    assert_eq!(
        receive_json(&mut recipient_socket).await["frame"]["sequence"],
        2
    );

    let lease: ControlLeaseOutput = client
        .post(format!(
            "{}/v1/terminal/rooms/{}/control",
            server.base_url, room.id
        ))
        .header("Authorization", bearer(&host_session.token))
        .json(&json!({
            "deviceId": refreshed_session.device_id,
            "durationSeconds": 30
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let input = signed_frame(
        &room,
        &refreshed_session,
        &recipient_device.signing,
        "input",
        1,
        Some(&lease),
        41,
    );
    recipient_socket
        .send(Message::Text(
            json!({ "type": "frame", "frame": input })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let host_input = receive_json(&mut host_socket).await;
    assert_eq!(host_input["frame"]["direction"], "input");
    assert_eq!(host_input["frame"]["sequence"], 1);
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            recipient_socket.next()
        )
        .await
        .is_err()
    );

    let duplicate = host_input["frame"].clone();
    recipient_socket
        .send(Message::Text(
            json!({ "type": "frame", "frame": duplicate })
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let rejection = receive_json(&mut recipient_socket).await;
    assert_eq!(rejection["type"], "error");
    assert_eq!(rejection["code"], "conflict");

    let removal = client
        .patch(format!(
            "{}/v1/workspaces/{}/members/{}",
            server.base_url, workspace_id, recipient_member_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .json(&json!({ "role": "operator", "status": "removed" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json::<Value>()
        .await
        .unwrap();
    assert_eq!(removal["keyEpoch"], 3);
    let revoked = client
        .get(format!(
            "{}/v1/workspaces/{}",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&refreshed_session.token))
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);

    let audit: Vec<RelayAuditEvent> = client
        .get(format!(
            "{}/v1/workspaces/{}/audit",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(audit.iter().any(|event| event.action == "member-updated"));
    let audit_json = serde_json::to_string(&audit).unwrap();
    assert!(!audit_json.contains("ciphertext"));
    assert!(!audit_json.contains("terminal-secret"));

    client
        .delete(format!(
            "{}/v1/workspaces/{}/permanent-deletion",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .header("X-CNshell-Confirm-Workspace", &workspace_id)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let deleted = client
        .get(format!(
            "{}/v1/workspaces/{}",
            server.base_url, workspace_id
        ))
        .header("Authorization", bearer(&host_session.token))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::UNAUTHORIZED);
}
