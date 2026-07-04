import os
import re

path = r'C:\Users\frimp\Documents\remem\rememhq-api\src\main.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# 1. Imports
imports = """
use validator::Validate;
use base64::{engine::general_purpose, Engine as _};

fn decode_cursor(cursor: Option<String>) -> usize {
    cursor.and_then(|c| {
        general_purpose::STANDARD.decode(c).ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .and_then(|s| s.parse::<usize>().ok())
    }).unwrap_or(0)
}

fn encode_cursor(offset: usize) -> String {
    general_purpose::STANDARD.encode(offset.to_string())
}

#[derive(Serialize, ToSchema)]
struct PaginatedResponse<T> {
    data: Vec<T>,
    next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema, Validate)]
struct ApiStoreRequest {
    #[validate(length(min = 1, message = "Content cannot be empty"))]
    pub content: String,
    
    #[validate(range(min = 1.0, max = 10.0, message = "Importance must be between 1.0 and 10.0"))]
    pub importance: Option<f32>,
    
    pub tags: Option<Vec<String>>,
    pub memory_type: Option<String>,
    pub ttl_days: Option<u32>,
}
"""
content = content.replace('use utoipa::ToSchema;\n', 'use utoipa::ToSchema;\n' + imports)

# 2. Derives and properties
content = content.replace('#[derive(Deserialize)]\nstruct RecallQuery {', '#[derive(Deserialize, Validate)]\nstruct RecallQuery {')
content = content.replace('#[derive(Deserialize)]\nstruct SearchQuery {', '#[derive(Deserialize, Validate)]\nstruct SearchQuery {')
content = content.replace('#[derive(Serialize, Deserialize, ToSchema)]\nstruct UpdateBody {', '#[derive(Serialize, Deserialize, ToSchema, Validate)]\nstruct UpdateBody {')
content = content.replace('#[derive(Deserialize)]\nstruct ListQuery {', '#[derive(Deserialize, Validate)]\nstruct ListQuery {')

content = re.sub(r'q: String,', '#[validate(length(min = 1))]\n    q: String,', content)
content = re.sub(r'offset: Option<usize>,', 'cursor: Option<String>,', content)

# 3. ListQuery cursor
content = content.replace('limit: usize,\n    #[serde(default)]\n    filter_tags: Option<String>,', 'limit: usize,\n    cursor: Option<String>,\n    #[serde(default)]\n    filter_tags: Option<String>,')

# 4. store_memory
content = content.replace('Json(req): Json<StoreRequest>', 'Json(req): Json<ApiStoreRequest>')
content = content.replace('request_body = StoreRequest,', 'request_body = ApiStoreRequest,')

store_valid = """    check_auth(&headers)?;
    if let Err(e) = req.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }"""
content = content.replace('    check_auth(&headers)?;\n\n    let auto_score = req.importance.is_none();', store_valid + '\n\n    let auto_score = req.importance.is_none();')

content = content.replace('let mut record = MemoryRecord::new(&req.content, req.memory_type).with_tags(req.tags);', 
'''let memory_type = req.memory_type.and_then(|s| s.parse().ok()).unwrap_or(MemoryType::Fact);
    let mut record = MemoryRecord::new(&req.content, memory_type).with_tags(req.tags.unwrap_or_default());''')


# 5. recall_memories
recall_sig = '-> Result<Json<Vec<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {'
recall_new_sig = '-> Result<Json<PaginatedResponse<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {'
content = content.replace(recall_sig, recall_new_sig)

recall_valid = """    check_auth(&headers)?;
    if let Err(e) = q.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }"""
content = content.replace('    check_auth(&headers)?;\n\n    let filter_tags', recall_valid + '\n\n    let filter_tags')

content = content.replace('let offset = q.offset.unwrap_or(0);', 'let offset = decode_cursor(q.cursor);')

recall_ret = """    let paginated = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_cursor = if paginated.len() == limit { Some(encode_cursor(offset + limit)) } else { None };
    Ok(Json(PaginatedResponse { data: paginated, next_cursor }))"""
content = re.sub(r'    let paginated = results\n        \.into_iter\(\)\n        \.skip\(offset\)\n        \.take\(limit\)\n        \.collect::<Vec<_>>\(\);\n    Ok\(Json\(paginated\)\)', recall_ret, content)

content = content.replace('body = Vec<MemoryResult>', 'body = PaginatedResponse<MemoryResult>')
content = content.replace('("offset" = Option<usize>,', '("cursor" = Option<String>,')

# 6. search_memories
search_sig = '-> Result<Json<Vec<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {'
search_new_sig = '-> Result<Json<PaginatedResponse<MemoryResult>>, (StatusCode, Json<ErrorResponse>)> {'
# Only replace the second occurrence, which is in search_memories (the first was in recall, already replaced)
content = content.replace(search_sig, search_new_sig)

search_valid = """    check_auth(&headers)?;
    if let Err(e) = q.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }"""
content = content.replace('    check_auth(&headers)?;\n\n    let filter_tags: Vec<String> = q', search_valid + '\n\n    let filter_tags: Vec<String> = q')

# offset already replaced because they had identical "let offset = q.offset.unwrap_or(0);" but wait, I replaced string literally.
# Let's verify string replacement worked for both.
# The previous string replace was 'let offset = q.offset.unwrap_or(0);' -> 'let offset = decode_cursor(q.cursor);' so it replaced all occurrences!

search_ret = """    let paginated = results
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let next_cursor = if paginated.len() == limit { Some(encode_cursor(offset + limit)) } else { None };
    Ok(Json(PaginatedResponse { data: paginated, next_cursor }))"""
# Same replacement for return
content = re.sub(r'    let paginated = results\n        \.into_iter\(\)\n        \.skip\(offset\)\n        \.take\(limit\)\n        \.collect::<Vec<_>>\(\);\n    Ok\(Json\(paginated\)\)', search_ret, content)

# 7. update_memory
update_valid = """    check_auth(&headers)?;
    if let Err(e) = body.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }"""
content = content.replace('    check_auth(&headers)?;\n\n    let id = uuid::Uuid::parse_str(&id)', update_valid + '\n\n    let id = uuid::Uuid::parse_str(&id)')

# 8. list_memories
list_memories_sig = '-> Result<Json<Vec<MemoryRecord>>, (StatusCode, Json<ErrorResponse>)> {'
list_memories_new_sig = '-> Result<Json<PaginatedResponse<MemoryRecord>>, (StatusCode, Json<ErrorResponse>)> {'
content = content.replace(list_memories_sig, list_memories_new_sig)

list_mem_valid = """    if let Err(e) = q.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }
    let offset = decode_cursor(q.cursor);"""
content = content.replace('    let filter_tags: Vec<String> = q', list_mem_valid + '\n    let filter_tags: Vec<String> = q')

content = content.replace('body = Vec<MemoryRecord>', 'body = PaginatedResponse<MemoryRecord>')

# update list_memories fetch logic
list_mem_fetch = """        Ok(memories) => {
            let paginated = memories.into_iter().skip(offset).take(q.limit).collect::<Vec<_>>();
            let next_cursor = if paginated.len() == q.limit { Some(encode_cursor(offset + q.limit)) } else { None };
            Ok(Json(PaginatedResponse { data: paginated, next_cursor }))
        }"""
content = re.sub(r'        Ok\(memories\) => Ok\(Json\(memories\)\),', list_mem_fetch, content)

# 9. list_sessions
list_sess_sig = '-> Result<Json<Vec<SessionResponse>>, (StatusCode, Json<ErrorResponse>)> {'
list_sess_new_sig = '-> Result<Json<PaginatedResponse<SessionResponse>>, (StatusCode, Json<ErrorResponse>)> {'
content = content.replace(list_sess_sig, list_sess_new_sig)

list_sess_valid = """    if let Err(e) = q.validate() {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: e.to_string() })));
    }
    let offset = decode_cursor(q.cursor);"""
content = content.replace('match engine.list_sessions(q.limit).await {', list_sess_valid + '\n    match engine.list_sessions(offset + q.limit).await {')

content = content.replace('body = Vec<SessionResponse>', 'body = PaginatedResponse<SessionResponse>')

list_sess_fetch = """        Ok(sessions) => {
            let paginated = sessions
                .into_iter()
                .skip(offset)
                .take(q.limit)
                .map(|r| SessionResponse {
                    id: r.id,
                    project: r.project,
                    started_at: r.started_at,
                    ended_at: r.ended_at,
                    consolidated: r.consolidated,
                    memory_count: r.memory_count,
                })
                .collect::<Vec<_>>();
            let next_cursor = if paginated.len() == q.limit { Some(encode_cursor(offset + q.limit)) } else { None };
            Ok(Json(PaginatedResponse { data: paginated, next_cursor }))
        }"""
content = re.sub(r'        Ok\(sessions\) => \{\n            let res = sessions\n                \.into_iter\(\)\n                \.map\(\|r\| SessionResponse \{\n                    id: r\.id,\n                    project: r\.project,\n                    started_at: r\.started_at,\n                    ended_at: r\.ended_at,\n                    consolidated: r\.consolidated,\n                    memory_count: r\.memory_count,\n                \}\)\n                \.collect\(\);\n            Ok\(Json\(res\)\)\n        \}', list_sess_fetch, content)


# 10. health
health_old = """async fn health() -> &'static str {
    "ok"
}"""
health_new = """#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "Service is healthy", body = serde_json::Value),
        (status = 503, description = "Service unavailable", body = serde_json::Value)
    )
)]
async fn health(State(engine): State<AppState>) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match engine.store.stats().await {
        Ok(_) => Ok(Json(serde_json::json!({ "status": "ok", "db": "connected" }))),
        Err(_) => Err((
            StatusCode::SERVICE_UNAVAILABLE, 
            Json(serde_json::json!({ "status": "error", "db": "disconnected" }))
        ))
    }
}"""
content = content.replace(health_old, health_new)

# 11. ApiDoc
content = content.replace('StoreRequest,', 'ApiStoreRequest,\n            PaginatedResponse<MemoryResult>,\n            PaginatedResponse<MemoryRecord>,\n            PaginatedResponse<SessionResponse>,')
content = content.replace('paths(\n        store_memory,', 'paths(\n        health,\n        store_memory,')

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print("done")
