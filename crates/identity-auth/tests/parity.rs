//! design D8 的「值对象等价语料」：`identity-auth` 的绑定值对象与协议侧同名值对象
//! 必须对**同一份接受/拒绝语料**给出**相同**结论（同为接受或同为拒绝）。
//!
//! 为什么需要：`identity-auth` 按 §5 依赖矩阵不得复用协议 crate 的业务类型，因此
//! `CanonicalOrigin` / `NodeEndpoint` 在两侧各有一份校验逻辑。这份语料把「两份逻辑等价」
//! 变成可执行判据——任何一侧放宽或收紧都会立刻失败（例如一侧接受大写主机而另一侧拒绝）。
//!
//! 语料按 design D8 列出的边界构造：带路径/查询/fragment 的 origin、非 `https`/`wss` 方案、
//! 大写主机、端口、IPv6 字面量、空串、超长串、尾随点。

/// 一个语料项：同一输入必须让两侧得到同一结论。
struct Corpus {
    input: &'static str,
    /// 便于报告：期望两侧都接受还是都拒绝（`None` = 只要求两侧一致）。
    expected: Option<bool>,
    note: &'static str,
}

/// origin 语料（`identity-auth::CanonicalOrigin` ↔ `sync_protocol::pairing::CanonicalOrigin`）。
const ORIGINS: &[Corpus] = &[
    Corpus {
        input: "https://work-pc.example.ts.net",
        expected: Some(true),
        note: "典型 Tailscale 主机名",
    },
    Corpus {
        input: "https://work-pc.example.ts.net:8443",
        expected: Some(true),
        note: "带端口",
    },
    Corpus {
        input: "https://[fd7a:115c:a1e0::1]:443",
        expected: Some(true),
        note: "IPv6 字面量",
    },
    Corpus {
        input: "https://WORK-PC.example.ts.net",
        expected: Some(true),
        note: "大写主机：两侧都不做大小写归一（保持一致即可）",
    },
    Corpus {
        input: "https://work-pc.example.ts.net/",
        expected: Some(false),
        note: "带路径",
    },
    Corpus {
        input: "https://work-pc.example.ts.net/?a=1",
        expected: Some(false),
        note: "带查询",
    },
    Corpus {
        input: "https://work-pc.example.ts.net/#frag",
        expected: Some(false),
        note: "带 fragment",
    },
    Corpus {
        input: "http://work-pc.example.ts.net",
        expected: Some(false),
        note: "非 https 方案",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net",
        expected: Some(false),
        note: "https 的位置给 wss",
    },
    Corpus {
        input: "https://",
        expected: Some(false),
        note: "空 authority",
    },
    Corpus {
        input: "",
        expected: Some(false),
        note: "空串",
    },
    Corpus {
        input: "https://work-pc.example.ts.net.",
        expected: Some(true),
        note: "尾随点：两侧都按字面接受，不做 DNS 归一（保持一致即可）",
    },
    Corpus {
        input: "https://work pc.example.ts.net",
        expected: Some(false),
        note: "authority 内含空白",
    },
    Corpus {
        input: "https://\u{7}example.ts.net",
        expected: Some(false),
        note: "authority 内含控制字符",
    },
];

/// endpoint 语料（`identity-auth::NodeEndpoint` ↔ `node_link_protocol::pairing::Endpoint`）。
const ENDPOINTS: &[Corpus] = &[
    Corpus {
        input: "wss://work-pc.example.ts.net/node-link/v1",
        expected: Some(true),
        note: "规范形态",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net:8443/node-link/v1",
        expected: Some(true),
        note: "带端口",
    },
    Corpus {
        input: "wss://[fd7a:115c:a1e0::1]:8443/node-link/v1",
        expected: Some(true),
        note: "IPv6 字面量",
    },
    Corpus {
        input: "https://work-pc.example.ts.net/node-link/v1",
        expected: Some(false),
        note: "非 wss 方案",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net",
        expected: Some(false),
        note: "缺固定路径",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net/node-link/v2",
        expected: Some(false),
        note: "版本路径不匹配",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net/node-link/v1/extra",
        expected: Some(false),
        note: "路径后有额外段",
    },
    Corpus {
        input: "wss://work-pc.example.ts.net/node-link/v1?x=1",
        expected: Some(false),
        note: "带查询",
    },
    Corpus {
        input: "wss:///node-link/v1",
        expected: Some(false),
        note: "空 authority",
    },
    Corpus {
        input: "wss://work pc.example.ts.net/node-link/v1",
        expected: Some(false),
        note: "authority 内含空白",
    },
    Corpus {
        input: "",
        expected: Some(false),
        note: "空串",
    },
];

/// 超长输入（> 2048 字符）：两侧都必须拒绝（上限是 wire 合同的一部分）。
fn overlong_origin() -> String {
    format!("https://{}.example.ts.net", "a".repeat(2048))
}

/// 超长 endpoint：同上。
fn overlong_endpoint() -> String {
    format!("wss://{}.example.ts.net/node-link/v1", "a".repeat(2048))
}

#[test]
fn canonical_origin_accepts_and_rejects_exactly_like_the_sync_protocol_type() {
    let mut mismatches = Vec::new();
    for case in ORIGINS {
        let ours = identity_auth::CanonicalOrigin::parse(case.input);
        let theirs = sync_protocol::pairing::CanonicalOrigin::parse(case.input);
        assert_eq!(
            ours.is_ok(),
            theirs.is_ok(),
            "origin 语料不一致：{:?}（{}）：identity-auth={:?}，sync-protocol={:?}",
            case.input,
            case.note,
            ours.as_ref().map(identity_auth::CanonicalOrigin::as_str),
            theirs.as_ref().map(|value| value.as_str())
        );
        if let Some(expected) = case.expected {
            assert_eq!(
                ours.is_ok(),
                expected,
                "origin 语料期望值不符：{:?}（{}）",
                case.input,
                case.note
            );
        }
        // 接受时文本必须逐字保留（不做归一化）；两侧口径也一致。
        if let (Ok(ours), Ok(theirs)) = (&ours, &theirs) {
            if ours.as_str() != theirs.as_str() {
                mismatches.push(case.input);
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "接受语义不一致（保留文本不同）：{mismatches:?}"
    );

    let long = overlong_origin();
    assert!(
        identity_auth::CanonicalOrigin::parse(&long).is_err()
            && sync_protocol::pairing::CanonicalOrigin::parse(&long).is_err(),
        "超长 origin 两侧都必须拒绝"
    );
}

#[test]
fn node_endpoint_accepts_and_rejects_exactly_like_the_node_link_protocol_type() {
    let mut mismatches = Vec::new();
    for case in ENDPOINTS {
        let ours = identity_auth::NodeEndpoint::parse(case.input);
        let theirs = node_link_protocol::pairing::Endpoint::parse(case.input);
        assert_eq!(
            ours.is_ok(),
            theirs.is_ok(),
            "endpoint 语料不一致：{:?}（{}）：identity-auth={:?}，node-link-protocol={:?}",
            case.input,
            case.note,
            ours.as_ref().map(identity_auth::NodeEndpoint::as_str),
            theirs.as_ref().map(|value| value.as_str())
        );
        if let Some(expected) = case.expected {
            assert_eq!(
                ours.is_ok(),
                expected,
                "endpoint 语料期望值不符：{:?}（{}）",
                case.input,
                case.note
            );
        }
        if let (Ok(ours), Ok(theirs)) = (&ours, &theirs) {
            if ours.as_str() != theirs.as_str() {
                mismatches.push(case.input);
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "接受语义不一致（保留文本不同）：{mismatches:?}"
    );

    let long = overlong_endpoint();
    assert!(
        identity_auth::NodeEndpoint::parse(&long).is_err()
            && node_link_protocol::pairing::Endpoint::parse(&long).is_err(),
        "超长 endpoint 两侧都必须拒绝"
    );
}
