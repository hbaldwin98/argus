use serde::{de::DeserializeOwned, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

const MAX_FRAME: u32 = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FramingError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("encode error: {0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("decode error: {0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("frame of {0} bytes exceeds max {MAX_FRAME}")]
    TooLarge(u32),
}

pub async fn write_msg<W, T>(w: &mut W, msg: &T) -> Result<(), FramingError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let buf = rmp_serde::to_vec_named(msg)?;
    let len = buf.len() as u32;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&buf).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_msg<R, T>(r: &mut R) -> Result<T, FramingError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf);
    if len > MAX_FRAME {
        return Err(FramingError::TooLarge(len));
    }
    let mut buf = vec![0u8; len as usize];
    r.read_exact(&mut buf).await?;
    Ok(rmp_serde::from_slice(&buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Cell, CellSpan, Cursor};
    use crate::ids::{CheckoutId, PaneId};
    use crate::message::{ClientMsg, ServerMsg};
    use crate::tree::{CheckoutInfo, GitStatus, PaneInfo, PaneKind, PaneStatus, ProjectInfo};
    use crate::ProjectId;

    async fn roundtrip<T>(msg: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
    {
        let mut buf: Vec<u8> = Vec::new();
        write_msg(&mut buf, msg).await.unwrap();
        read_msg(&mut buf.as_slice()).await.unwrap()
    }

    #[tokio::test]
    async fn client_messages_survive_the_wire() {
        let sent = ClientMsg::Input {
            pane: PaneId(9),
            bytes: b"echo hello\r".to_vec(),
        };
        let ClientMsg::Input { pane, bytes } = roundtrip(&sent).await else {
            panic!("variant changed across the wire");
        };
        assert_eq!(pane, PaneId(9));
        assert_eq!(bytes, b"echo hello\r");
    }

    #[tokio::test]
    async fn every_client_variant_roundtrips() {
        let msgs = vec![
            ClientMsg::Subscribe { pane: PaneId(1) },
            ClientMsg::Unsubscribe { pane: PaneId(1) },
            ClientMsg::Input {
                pane: PaneId(1),
                bytes: vec![0, 27, 255],
            },
            ClientMsg::Resize {
                pane: PaneId(1),
                rows: 40,
                cols: 120,
            },
            ClientMsg::SpawnShell { checkout: CheckoutId(2) },
            ClientMsg::SpawnAgent {
                checkout: CheckoutId(2),
                template: "claude".to_string(),
            },
            ClientMsg::Kill { pane: PaneId(1) },
            ClientMsg::CreateWorktree {
                checkout: CheckoutId(2),
                branch: "feat/x".to_string(),
            },
            ClientMsg::RemoveCheckout { checkout: CheckoutId(2) },
            ClientMsg::AddProject {
                path: r"C:\src\thing".to_string(),
            },
        ];
        for msg in &msgs {
            let back = roundtrip(msg).await;
            assert_eq!(
                std::mem::discriminant(msg),
                std::mem::discriminant(&back),
                "variant mismatch for {msg:?}"
            );
            assert_eq!(format!("{msg:?}"), format!("{back:?}"));
        }
    }

    #[tokio::test]
    async fn a_tree_snapshot_survives_the_wire() {
        let sent = ServerMsg::Tree(vec![ProjectInfo {
            id: ProjectId(1),
            name: "argus".to_string(),
            checkouts: vec![CheckoutInfo {
                id: CheckoutId(2),
                name: "master".to_string(),
                path: "C:/src/argus".to_string(),
                primary: true,
                git: Some(GitStatus {
                    branch: Some("master".to_string()),
                    dirty: true,
                    changed_files: 3,
                    ahead: 1,
                    behind: 2,
                }),
                panes: vec![PaneInfo {
                    id: PaneId(3),
                    kind: PaneKind::Agent,
                    title: "claude".to_string(),
                    status: PaneStatus::Waiting,
                    note: None,
                    template: None,
                }],
            }],
        }]);
        let ServerMsg::Tree(tree) = roundtrip(&sent).await else {
            panic!("variant changed across the wire");
        };
        let c = &tree[0].checkouts[0];
        assert!(c.primary);
        assert_eq!(c.git.as_ref().unwrap().changed_files, 3);
        assert_eq!(c.panes[0].status, PaneStatus::Waiting);
    }

    #[tokio::test]
    async fn every_pane_status_survives_the_wire() {
        for status in [
            PaneStatus::Idle,
            PaneStatus::Working,
            PaneStatus::Waiting,
            PaneStatus::Exited { code: Some(0) },
            PaneStatus::Exited { code: Some(1) },
            PaneStatus::Exited { code: None },
        ] {
            let mut buf: Vec<u8> = Vec::new();
            write_msg(&mut buf, &status).await.unwrap();
            let back: PaneStatus = read_msg(&mut buf.as_slice()).await.unwrap();
            assert_eq!(back, status);
        }
    }

    #[tokio::test]
    async fn damage_spans_survive_the_wire() {
        let sent = ServerMsg::Damage {
            pane: PaneId(1),
            cursor: Cursor {
                row: 4,
                col: 8,
                visible: true,
            },
            spans: vec![CellSpan {
                row: 4,
                col: 7,
                cells: vec![Cell {
                    ch: "é".to_string(),
                    bold: true,
                    ..Default::default()
                }],
            }],
        };
        let ServerMsg::Damage { spans, cursor, .. } = roundtrip(&sent).await else {
            panic!("variant changed across the wire");
        };
        assert_eq!(spans[0].cells[0].ch, "é", "non-ascii must survive");
        assert!(spans[0].cells[0].bold);
        assert_eq!(cursor, Cursor { row: 4, col: 8, visible: true });
    }

    #[tokio::test]
    async fn back_to_back_frames_read_in_order() {
        // The transport is a stream, not a datagram socket: framing has to
        // keep messages separated when several are written before any read.
        let mut buf: Vec<u8> = Vec::new();
        for pane in 1..=3u64 {
            write_msg(&mut buf, &ClientMsg::Subscribe { pane: PaneId(pane) }).await.unwrap();
        }
        let mut r = buf.as_slice();
        for pane in 1..=3u64 {
            let msg: ClientMsg = read_msg(&mut r).await.unwrap();
            let ClientMsg::Subscribe { pane: got } = msg else {
                panic!("wrong variant")
            };
            assert_eq!(got, PaneId(pane));
        }
    }

    #[tokio::test]
    async fn an_oversized_length_prefix_is_rejected_before_allocating() {
        // Without this guard a bogus prefix would `vec![0; 4GB]`.
        let mut buf = (MAX_FRAME + 1).to_be_bytes().to_vec();
        buf.extend_from_slice(b"whatever");
        let err = read_msg::<_, ClientMsg>(&mut buf.as_slice()).await.unwrap_err();
        assert!(matches!(err, FramingError::TooLarge(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_truncated_frame_is_an_io_error_not_a_hang() {
        let mut buf: Vec<u8> = Vec::new();
        write_msg(&mut buf, &ClientMsg::Kill { pane: PaneId(1) }).await.unwrap();
        buf.truncate(buf.len() - 1);
        let err = read_msg::<_, ClientMsg>(&mut buf.as_slice()).await.unwrap_err();
        assert!(matches!(err, FramingError::Io(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_closed_stream_is_an_io_error() {
        let err = read_msg::<_, ClientMsg>(&mut [].as_slice()).await.unwrap_err();
        assert!(matches!(err, FramingError::Io(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn garbage_payload_is_a_decode_error() {
        let mut buf = 3u32.to_be_bytes().to_vec();
        buf.extend_from_slice(&[0xff, 0xff, 0xff]);
        let err = read_msg::<_, ClientMsg>(&mut buf.as_slice()).await.unwrap_err();
        assert!(matches!(err, FramingError::Decode(_)), "got {err:?}");
    }
}
