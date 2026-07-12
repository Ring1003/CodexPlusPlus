// 会话管理分页逻辑的单元测试。
// 聚焦 paginate_local_sessions 纯函数：关键词过滤、排序、分页切片、total 统计。
// 这些逻辑是「会话管理 tab 性能优化」的核心，900+ 会话下保证分页正确性。

use codex_plus_data::{LocalSession, paginate_local_sessions};

/// 构造一条 LocalSession（updated_at_ms 从 1 开始递增，便于断言排序）。
fn make_session(id: &str, title: &str, cwd: &str, provider: &str, ts: i64) -> LocalSession {
    LocalSession {
        id: id.to_string(),
        title: title.to_string(),
        cwd: cwd.to_string(),
        model_provider: provider.to_string(),
        archived: false,
        updated_at_ms: Some(ts),
        rollout_path: String::new(),
        db_path: String::new(),
    }
}

/// 构造 N 条会话，ts 从 N 递减到 1（模拟倒序），id 形如 "s001".."sNNN"。
fn make_sessions(n: usize) -> Vec<LocalSession> {
    (0..n)
        .map(|i| {
            let ts = (n - i) as i64; // s000.ts=N, s001.ts=N-1, ... 倒序
            make_session(
                &format!("s{i:03}"),
                &format!("Session {i}"),
                &format!("/proj/{i}"),
                "openai",
                ts,
            )
        })
        .collect()
}

#[test]
fn first_page_returns_limit_items_in_desc_order() {
    let sessions = make_sessions(120);
    // 第一页：limit=50, offset=0
    let result = paginate_local_sessions(sessions, 50, 0, None);
    // total 应反映全集
    assert_eq!(result.total, 120);
    // 当页 50 条
    assert_eq!(result.sessions.len(), 50);
    // 倒序：updated_at_ms 最新的在前。make_sessions 里 s000.ts=120 最大，应在首位
    assert_eq!(result.sessions[0].id, "s000");
    assert_eq!(result.sessions[0].updated_at_ms, Some(120));
    // 第 50 条应是 s049（ts=71）
    assert_eq!(result.sessions[49].id, "s049");
}

#[test]
fn second_page_uses_offset_correctly() {
    let sessions = make_sessions(120);
    let result = paginate_local_sessions(sessions, 50, 50, None);
    assert_eq!(result.total, 120);
    assert_eq!(result.sessions.len(), 50);
    // 第二页从 s050 开始（ts=70）
    assert_eq!(result.sessions[0].id, "s050");
    assert_eq!(result.sessions[0].updated_at_ms, Some(70));
}

#[test]
fn last_page_returns_only_remaining_items() {
    let sessions = make_sessions(120);
    // 第三页：offset=100，剩余 20 条
    let result = paginate_local_sessions(sessions, 50, 100, None);
    assert_eq!(result.total, 120);
    assert_eq!(result.sessions.len(), 20);
    assert_eq!(result.sessions[0].id, "s100");
    assert_eq!(result.sessions[19].id, "s119");
}

#[test]
fn offset_beyond_total_returns_empty_but_keeps_total() {
    let sessions = make_sessions(30);
    // offset 远超 total
    let result = paginate_local_sessions(sessions, 50, 9999, None);
    assert_eq!(result.total, 30);
    assert!(result.sessions.is_empty());
}

#[test]
fn limit_zero_means_no_pagination_returns_all() {
    let sessions = make_sessions(75);
    // limit=0 表示不分页（向后兼容旧的全量调用）
    let result = paginate_local_sessions(sessions, 0, 0, None);
    assert_eq!(result.total, 75);
    assert_eq!(result.sessions.len(), 75);
    // 仍按 updated_at_ms desc 排序
    assert_eq!(result.sessions[0].id, "s000");
    assert_eq!(result.sessions[74].id, "s074");
}

#[test]
fn negative_limit_also_returns_all() {
    let sessions = make_sessions(10);
    let result = paginate_local_sessions(sessions, -1, 0, None);
    assert_eq!(result.total, 10);
    assert_eq!(result.sessions.len(), 10);
}

#[test]
fn query_matches_title_case_insensitive() {
    let mut sessions = make_sessions(100);
    // 给其中几条加上可搜索的 title
    sessions[3].title = "Important Meeting".to_string();
    sessions[7].title = "Another important task".to_string();
    sessions[50].cwd = "/important/path".to_string();

    // 搜索 "important"：应命中 3 条（两条 title + 一条 cwd）
    let result = paginate_local_sessions(sessions, 50, 0, Some("IMPORTANT"));
    assert_eq!(result.total, 3);
    assert_eq!(result.sessions.len(), 3);
    let ids: Vec<&str> = result.sessions.iter().map(|s| s.id.as_str()).collect();
    assert!(ids.contains(&"s003"));
    assert!(ids.contains(&"s007"));
    assert!(ids.contains(&"s050"));
}

#[test]
fn query_matches_cwd() {
    let sessions = make_sessions(100);
    // 搜索 cwd：/proj/5 应命中 s005（cwd="/proj/5"）
    let result = paginate_local_sessions(sessions, 50, 0, Some("/proj/5"));
    assert_eq!(result.total, 1);
    assert_eq!(result.sessions[0].id, "s005");
}

#[test]
fn query_matches_model_provider() {
    let mut sessions = make_sessions(20);
    sessions[5].model_provider = "anthropic-claude".to_string();
    sessions[15].model_provider = "anthropic-claude".to_string();

    let result = paginate_local_sessions(sessions, 50, 0, Some("anthropic"));
    assert_eq!(result.total, 2);
}

#[test]
fn query_blank_string_means_no_filter() {
    let sessions = make_sessions(60);
    // trim 后为空，应等同于 None
    let result = paginate_local_sessions(sessions, 10, 0, Some("   "));
    assert_eq!(result.total, 60);
    assert_eq!(result.sessions.len(), 10);
}

#[test]
fn query_no_match_returns_empty() {
    let sessions = make_sessions(60);
    let result = paginate_local_sessions(sessions, 50, 0, Some("nonexistent_keyword_xyz"));
    assert_eq!(result.total, 0);
    assert!(result.sessions.is_empty());
}

#[test]
fn query_combines_with_pagination() {
    let mut sessions = make_sessions(100);
    // 让前 75 条的 provider 都是 "deepseek"，后 25 条是 "openai"
    for s in sessions.iter_mut().take(75) {
        s.model_provider = "deepseek".to_string();
    }
    // 搜索 deepseek，total 应为 75，分页第 2 页（offset=50）应剩 25 条
    let result = paginate_local_sessions(sessions, 50, 50, Some("deepseek"));
    assert_eq!(result.total, 75);
    assert_eq!(result.sessions.len(), 25);
    // 每条都是 deepseek
    assert!(result.sessions.iter().all(|s| s.model_provider == "deepseek"));
}

#[test]
fn sort_by_id_desc_when_updated_at_ms_equal() {
    let sessions = vec![
        make_session("bbb", "B", "/p", "x", 100),
        make_session("aaa", "A", "/p", "x", 100),
        make_session("ccc", "C", "/p", "x", 100),
    ];
    let result = paginate_local_sessions(sessions, 10, 0, None);
    // ts 相同，按 id desc：ccc, bbb, aaa
    assert_eq!(result.sessions[0].id, "ccc");
    assert_eq!(result.sessions[1].id, "bbb");
    assert_eq!(result.sessions[2].id, "aaa");
}

#[test]
fn null_updated_at_ms_sorts_last() {
    let mut sessions = make_sessions(3);
    sessions[1].updated_at_ms = None; // s001 的 ts 置空
    let result = paginate_local_sessions(sessions, 10, 0, None);
    // s000(ts=3) 和 s002(ts=1) 在前，s001(null) 在最后
    assert_eq!(result.sessions[0].id, "s000");
    assert_eq!(result.sessions[1].id, "s002");
    assert_eq!(result.sessions[2].id, "s001");
}

#[test]
fn empty_input_returns_empty() {
    let result = paginate_local_sessions(Vec::new(), 50, 0, None);
    assert_eq!(result.total, 0);
    assert!(result.sessions.is_empty());
}
