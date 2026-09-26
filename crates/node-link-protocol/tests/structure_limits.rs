//! §2.5 的三个固定 JSON 结构上限（嵌套深度 64 / 单对象字段数 1,024 / 单数组元素数 10,000）。
//!
//! 每条上限都在**边界上取正例、越界取反例**，全部经 wire DTO 边界（`Envelope::decode`）判定：这三个上限
//! 不可下调、也不参与协商，适配器（`server::node_link`）只能把越界映射成
//! `nodelink.protocol.schema_invalid`，因此判定本身必须发生在协议层。
//!
//! 结构用 `link.error` 帧承载：它的 `details` 是 §2.4 的开放扩展点，可以放任意结构；本文件只验证结构，
//! 不看 body 形状（body 形状由 `envelope_fixtures.rs` 的 manifest 用例覆盖）。

mod support;

use node_link_protocol::envelope::{Envelope, EnvelopeError};
use node_link_protocol::structure::{
    MAX_ARRAY_ELEMENTS, MAX_NESTING_DEPTH, MAX_OBJECT_FIELDS, StructureError,
};

/// `details` 之外的固定嵌套层数：信封对象（1）→ `body`（2）→ `details`（3）。
/// 因此 `details` 的值每再嵌一层数组，整帧深度加一。
const DEPTH_OUTSIDE_DETAILS: usize = 3;

/// 一条 `link.error` 帧，`details` 的值由调用方给出。
fn frame(details: &str) -> String {
    format!(
        "{{\"protocolVersion\":1,\"type\":\"link.error\",\
         \"messageId\":\"3a4b5c6d-7e80-4192-a3b4-c5d6e7f8091a\",\
         \"body\":{{\"code\":\"nodelink.protocol.invalid_json\",\"message\":\"x\",\
         \"retryable\":false,\"correlationId\":null,\"details\":{details}}}}}"
    )
}

/// `details = {"deep": [[…]]}`：`levels` 层嵌套数组（整帧深度 = [`DEPTH_OUTSIDE_DETAILS`] + `levels`）。
fn frame_with_nested_arrays(levels: usize) -> String {
    frame(&format!(
        "{{\"deep\": {}{} }}",
        "[".repeat(levels),
        "]".repeat(levels)
    ))
}

/// `details = {"deep": [[…<字符串>…]]}`：`levels` 层嵌套数组，最内层是一个含括号与转义引号的字符串元素。
fn frame_with_nested_arrays_of_strings(levels: usize) -> String {
    frame(&format!(
        "{{\"deep\": {}{}{} }}",
        "[".repeat(levels),
        "\"[{\\\"]\"",
        "]".repeat(levels)
    ))
}

/// `details = {"f0":0,"f1":1,…}`：恰好 `count` 个成员的对象。
fn frame_with_object_members(count: usize) -> String {
    let members: Vec<String> = (0..count)
        .map(|index| format!("\"f{index}\":{index}"))
        .collect();
    frame(&format!("{{{}}}", members.join(",")))
}

/// `details = {"items": [0,1,…]}`：恰好 `count` 个元素的数组。
fn frame_with_array_elements(count: usize) -> String {
    frame(&format!("{{\"items\": {}}}", array_text(count)))
}

/// `[0,1,…]`：`count` 个元素的数组文本。
fn array_text(count: usize) -> String {
    let items: Vec<String> = (0..count).map(|index| index.to_string()).collect();
    format!("[{}]", items.join(","))
}

/// 期望的结构错误（同时锁住报告的实际值，避免把越界的数字写错）。
fn structure_error(text: &str) -> StructureError {
    match Envelope::decode(text) {
        Err(EnvelopeError::Structure(violation)) => violation,
        other => panic!("必须按 §2.5 的结构上限被拒，实际：{other:?}"),
    }
}

#[test]
fn nesting_depth_is_enforced_at_sixty_four() {
    assert_eq!(MAX_NESTING_DEPTH, 64);
    // 正例：整帧恰好 64 层，且 body 字节仍逐字节保留（结构判定不解析、不改写）。
    let at_limit = frame_with_nested_arrays(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS);
    let envelope = Envelope::decode(&at_limit).expect("恰好 64 层必须被接受");
    assert_eq!(envelope.body().get(), support::body_slice(&at_limit));

    // 反例：多一层。
    let over = frame_with_nested_arrays(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS + 1);
    assert_eq!(
        structure_error(&over),
        StructureError::NestingDepth {
            found: MAX_NESTING_DEPTH + 1,
            limit: MAX_NESTING_DEPTH,
        }
    );

    // 浅帧不受影响（上限不是「一律拒绝」）。
    assert!(Envelope::decode(&frame("{}")).is_ok());
}

#[test]
fn object_field_count_is_enforced_at_one_thousand_twenty_four() {
    assert_eq!(MAX_OBJECT_FIELDS, 1_024);
    let at_limit = frame_with_object_members(MAX_OBJECT_FIELDS);
    let envelope = Envelope::decode(&at_limit).expect("恰好 1,024 个字段必须被接受");
    assert_eq!(envelope.body().get(), support::body_slice(&at_limit));

    let over = frame_with_object_members(MAX_OBJECT_FIELDS + 1);
    assert_eq!(
        structure_error(&over),
        StructureError::ObjectFields {
            found: MAX_OBJECT_FIELDS + 1,
            limit: MAX_OBJECT_FIELDS,
        }
    );
}

#[test]
fn array_element_count_is_enforced_at_ten_thousand() {
    assert_eq!(MAX_ARRAY_ELEMENTS, 10_000);
    let at_limit = frame_with_array_elements(MAX_ARRAY_ELEMENTS);
    let envelope = Envelope::decode(&at_limit).expect("恰好 10,000 个元素必须被接受");
    assert_eq!(envelope.body().get(), support::body_slice(&at_limit));

    let over = frame_with_array_elements(MAX_ARRAY_ELEMENTS + 1);
    assert_eq!(
        structure_error(&over),
        StructureError::ArrayElements {
            found: MAX_ARRAY_ELEMENTS + 1,
            limit: MAX_ARRAY_ELEMENTS,
        }
    );
}

#[test]
fn counting_is_per_container_not_cumulative() {
    assert_eq!(MAX_OBJECT_FIELDS / 2, 512);
    assert_eq!(MAX_ARRAY_ELEMENTS / 2, 5_000);
    // 两个对象各 512 个字段、两个数组各 5,000 个元素：相加都超过单个上限，但每个容器都在范围内 → 接受。
    let wide = frame(&format!(
        "{{\"a\": {}, \"b\": {}}}",
        object_text(MAX_OBJECT_FIELDS / 2),
        object_text(MAX_OBJECT_FIELDS / 2)
    ));
    let long = frame(&format!(
        "{{\"x\": {}, \"y\": {}}}",
        array_text(MAX_ARRAY_ELEMENTS / 2),
        array_text(MAX_ARRAY_ELEMENTS / 2)
    ));
    assert!(Envelope::decode(&wide).is_ok(), "字段数按单个对象计");
    assert!(Envelope::decode(&long).is_ok(), "元素数按单个数组计");

    // 同一个对象里两种结构都顶到上限：各自独立计数，仍然接受。
    let mixed = frame(&format!(
        "{{\"fields\": {}, \"items\": {}}}",
        object_text(MAX_OBJECT_FIELDS),
        array_text(MAX_ARRAY_ELEMENTS)
    ));
    assert!(Envelope::decode(&mixed).is_ok());
}

/// `{"f0":0,…}`：`count` 个成员的纯对象文本。
fn object_text(count: usize) -> String {
    let members: Vec<String> = (0..count)
        .map(|index| format!("\"f{index}\":{index}"))
        .collect();
    format!("{{{}}}", members.join(","))
}

#[test]
fn strings_and_escapes_do_not_add_depth_or_members() {
    // 字符串里的 `[`/`{`/`"`（转义）既不增加嵌套深度、也不被当成容器或成员。
    let at_limit = frame_with_nested_arrays_of_strings(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS);
    assert!(
        Envelope::decode(&at_limit).is_ok(),
        "字符串内容不参与结构计数：{at_limit}"
    );

    // 反例对照：同样的构造多嵌一层数组就超限（证明上面的接受不是因为计数整体失效）。
    let over = frame_with_nested_arrays_of_strings(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS + 1);
    assert!(matches!(
        structure_error(&over),
        StructureError::NestingDepth {
            limit: MAX_NESTING_DEPTH,
            ..
        }
    ));
}

#[test]
fn the_limits_do_not_come_from_serde_defaults() {
    // `serde_json` 自己的递归上限默认是 128：65 层能通过它的解析，却必须被合同的 64 拒绝；
    // 单对象字段数与单数组元素数它完全不限制，因此只有本模块能判。
    let over_depth = frame_with_nested_arrays(MAX_NESTING_DEPTH - DEPTH_OUTSIDE_DETAILS + 1);
    assert!(
        serde_json::from_str::<serde_json::Value>(&over_depth).is_ok(),
        "该帧对 serde 是合法 JSON（65 < 128）"
    );
    assert!(matches!(
        structure_error(&over_depth),
        StructureError::NestingDepth { .. }
    ));

    let over_fields = frame_with_object_members(MAX_OBJECT_FIELDS + 1);
    assert!(serde_json::from_str::<serde_json::Value>(&over_fields).is_ok());
    assert!(matches!(
        structure_error(&over_fields),
        StructureError::ObjectFields { .. }
    ));

    let over_elements = frame_with_array_elements(MAX_ARRAY_ELEMENTS + 1);
    assert!(serde_json::from_str::<serde_json::Value>(&over_elements).is_ok());
    assert!(matches!(
        structure_error(&over_elements),
        StructureError::ArrayElements { .. }
    ));
}

#[test]
fn structure_is_checked_before_syntax() {
    // `Envelope::decode` 的顺序：结构上限先于语法判定（两者都是「消息不生效」，优先级写在函数文档里）。
    assert!(
        matches!(
            Envelope::decode("{not json"),
            Err(EnvelopeError::Malformed(_))
        ),
        "不含越界结构的畸形文本仍按语法错误报告"
    );
    let malformed_and_deep = format!("{}\"", "[".repeat(MAX_NESTING_DEPTH + 1));
    assert!(matches!(
        Envelope::decode(&malformed_and_deep),
        Err(EnvelopeError::Structure(
            StructureError::NestingDepth { .. }
        ))
    ));

    // 结构扫描不替代其它 wire 边界校验：越界的 `connectionSequence` 仍按 `decimalString` 判定。
    let bad_sequence = "{\"protocolVersion\":1,\"type\":\"link.ping\",\
                        \"messageId\":\"1728394a-5c6d-4e7f-88a9-b0c1d2e3f405\",\
                        \"connectionId\":\"2de54db7-74ae-4c21-88e3-05df0dc2e637\",\
                        \"connectionSequence\":\"01\",\
                        \"body\":{\"nonce\":\"AAECAwQFBgcICQoLDA0ODw\"}}";
    assert!(matches!(
        Envelope::decode(bad_sequence),
        Err(EnvelopeError::Malformed(_))
    ));
}
