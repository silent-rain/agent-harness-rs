-- =============================================================================
-- agent-assembly / examples / db_agent
-- 最小可运行 DB 配置脚本
-- =============================================================================
-- 适配三种引擎：MySQL 8+ / PostgreSQL / SQLite。
-- 字段含义与 schema 来自：
--   crates/core/migration/src/agent/{agent_config, llm_model, llm_model_rel,
--   rule_registry, rule_rel}.rs
--
-- 前置条件：
--   先用 service-hub 启动一次以触发 migration 建表，或在 migration crate 中执行：
--     cargo run -p migration -- up
--
-- 与 .env 的对应关系：
--   AGENT_ID=1   ↔ t_agent_config.id
--   USER_ID=1    ↔ t_agent_config.user_id
--
-- 执行方式（任选其一）：
--   MySQL:
--     mysql -uroot -proot agent_db < db_agent_seed.sql
--   PostgreSQL:
--     psql "postgres://user:pass@127.0.0.1:5432/agent_db" -f db_agent_seed.sql
--   SQLite:
--     sqlite3 path/to/db.sqlite < db_agent_seed.sql
-- =============================================================================
-- Insert into nucleus_agent
-- =============================================================================
-- 用户基础信息, 必填
-- 已内置了一个用户
-- =============================================================================
-- INSERT INTO
--     public.t_user_base (id, username, "password", enabled)
-- VALUES
--     (
--         1,
--         'SR',
--         'da023f7090dd831097f8a534475b1c4fba2a9a6419968e52be7459e2533ac819',
--         TRUE
--     );
-- =============================================================================
-- 智能体主体, 必填
-- =============================================================================
INSERT INTO
    public.t_agent_config (
        id,
        user_id,
        "name",
        DESCRIPTION,
        welcome_message,
        system_prompt,
        STATUS
    )
VALUES
    (
        1,
        1,
        'Qwen-Image 叙事提示词助手',
        '面向 Qwen-Image 2512 模型的叙事型提示词助手，将用户创意扩写为富有画面感的中文叙事提示词',
        '你好！我是 Qwen-Image 2512 叙事提示词助手，告诉我你想画的画面，我会为你生成富有故事感的中文叙事提示词。',
        '你是一名专业的 Qwen-Image 2512 提示词工程师，精通该模型对中文长文本与叙事化描述的理解能力。',
        1
    );

-- =============================================================================
-- LLM 模型, 必填
-- =============================================================================
-- LLM 模型
--    将 'sk-your-key-here' 替换成你自己的 API Key。
INSERT INTO
    public.t_agent_llm_model (
        id,
        user_id,
        provider,
        model_code,
        model_name,
        base_url,
        api_key,
        enabled
    )
VALUES
    (
        1,
        1,
        'qwen',
        'qwen3.5-plus',
        'qwen3.5-plus',
        'https://dashscope.aliyuncs.com/compatible-mode/v1',
        'sk-54e59e23249e42348ae6b7a90f154667',
        TRUE
    ),
    (
        2,
        1,
        'openai',
        'gpt-4o-mini',
        'GPT-4o mini',
        'https://api.openai.com/v1',
        'sk-your-key-here',
        TRUE
    );

-- 3) 模型绑定：将 model_id=1 绑定给 agent_id=1。
--    temperature / max_tokens 留空，由调用层兜底。
INSERT INTO
    t_agent_llm_model_rel (
        id,
        agent_id,
        model_id,
        temperature,
        max_tokens,
        enabled
    )
VALUES
    (1, 1, 1, NULL, NULL, TRUE);

-- =============================================================================
-- 可选：注册一条规则并绑定到 agent_id=1，便于演示 DbRuleLoader 输出非空。
-- 如果只想最小化运行，可以删掉下面两条 INSERT。
-- =============================================================================
INSERT INTO
    t_agent_rule_registry (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        content,
        enabled,
        deleted
    )
VALUES
    (
        1,
        1,
        'concise-answer',
        '保持回答简洁',
        '回答时尽量在 5 句话内表达清楚，避免冗长铺陈。',
        TRUE,
        FALSE
    );

INSERT INTO
    t_agent_rule_rel (id, agent_id, rule_id, priority, enabled)
VALUES
    (1, 1, 1, 100, TRUE);

-- =============================================================================
-- 可选：MCP 服务（注册表 + 绑定关系，两步插入）
-- 表：t_agent_mcp_service_registry —— 服务元数据与连接配置（per-user）
--   - user_id        : 0=平台内建，其他=用户私有
--   - name           : 服务名（同一用户下唯一）
--   - transport_type : 'stdio' / 'sse' / 'streamable-http'
--   - enabled_tools  : JSON 数组；'[]'=全部可用，'["tool1"]'=白名单
--   - config_json    : mcpServers 包裹格式，与本地 mcp.json 保持一致
-- 表：t_agent_mcp_service_rel —— 智能体↔MCP 服务绑定关系
--   - registry_id    : 外键 → t_agent_mcp_service_registry.id
--   - enabled_tools  : 覆盖注册表默认白名单；'[]'=使用注册表默认值
--
-- 不需要 MCP 时，可以删除下面两组 INSERT。
-- =============================================================================
-- Step 1：注册 MCP 服务（三种 transport 类型）
INSERT INTO
    t_agent_mcp_service_registry (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        mcp_service_url,
        transport_type,
        enabled_tools,
        config_json,
        enabled,
        deleted
    )
VALUES
    -- (1) stdio 类型：通过 uvx 拉起本地 MCP 子进程，env 注入 API Key
    (
        1,
        1,
        'fetch-mcp',
        '通过 uvx 拉起的本地 fetch MCP 子进程，支持网页内容抓取。',
        NULL,
        'stdio',
        '[]',
        '{"mcpServers":{"fetch-mcp":{"command":"uvx","args":["mcp-server-fetch"],"transportType":"stdio","env":{"API_KEY":"your-api-key"}}}}',
        TRUE,
        FALSE
    ),
    -- (2) sse 类型：Server-Sent Events 长连接
    (
        2,
        1,
        'sse-service',
        '基于 SSE 长连接的远程 MCP 服务，适合低延迟实时推送场景。',
        'http://127.0.0.1:8080/sse',
        'sse',
        '[]',
        '{"mcpServers":{"sse-service":{"url":"http://127.0.0.1:8080/sse","transportType":"sse","timeout":30000,"headers":{"Authorization":"Bearer your-token"}}}}',
        TRUE,
        FALSE
    ),
    -- (3) streamable-http 类型：标准 HTTP 流式传输，带自定义 Header
    (
        3,
        1,
        'http-service',
        '基于 Streamable HTTP 的远程 MCP 服务，支持自定义请求头鉴权。',
        'http://127.0.0.1:8081/mcp',
        'streamable-http',
        '[]',
        '{"mcpServers":{"http-service":{"url":"http://127.0.0.1:8081/mcp","transportType":"streamable-http","timeout":30000,"headers":{"X-Api-Key":"your-api-key"}}}}',
        TRUE,
        FALSE
    );

-- Step 2：将三个 MCP 服务绑定到 agent_id=1
--   enabled_tools='[]' 表示使用注册表默认白名单（全部可用）
INSERT INTO
    t_agent_mcp_service_rel (id, agent_id, registry_id, enabled_tools, enabled)
VALUES
    (1, 1, 1, '[]', TRUE),
    (2, 1, 2, '[]', TRUE),
    (3, 1, 3, '[]', TRUE);

-- =============================================================================
-- 可选：A2A 服务（注册表 + 绑定关系，两步插入）
-- 表：t_agent_a2a_service_registry —— 远程 Agent 服务元数据与连接配置（per-user）
--   - user_id      : 0=平台内建，其他=用户私有
--   - name         : 服务名（同一用户下唯一，LLM 可见）
--   - description  : 服务描述（LLM 可见，覆盖 AgentCard 中的 description）
--   - base_url     : 远程 A2A 服务基础 URL
--   - timeout_secs : 连接超时（秒），NULL 使用默认值 60
--   - config_json  : a2aServers 包裹格式，与本地 a2a.json 保持一致
-- 表：t_agent_a2a_service_rel —— 智能体↔A2A 服务绑定关系
--   - registry_id  : 外键 → t_agent_a2a_service_registry.id
--
-- 不需要 A2A 时，可以删除下面两组 INSERT。
-- =============================================================================
-- Step 1：注册 A2A 服务
INSERT INTO
    t_agent_a2a_service_registry (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        base_url,
        timeout_secs,
        config_json,
        enabled,
        deleted
    )
VALUES
    -- (1) 天气查询 Agent
    (
        1,
        1,
        'weather_agent',
        '天气查询 Agent，支持全球城市实时天气与未来 7 天预报查询。',
        'http://127.0.0.1:3001',
        60,
        '{"a2aServers":{"weather_agent":{"base_url":"http://127.0.0.1:3001","description":"Query weather information","timeout_secs":60,"headers":{"Authorization":"Bearer your-token"}}}}',
        TRUE,
        FALSE
    ),
    -- (2) 代码执行 Agent
    (
        2,
        1,
        'code_executor',
        '代码执行 Agent，支持在沙箱环境中运行 Python / JavaScript 代码片段并返回结果。',
        'http://127.0.0.1:3002',
        120,
        '{"a2aServers":{"code_executor":{"base_url":"http://127.0.0.1:3002","description":"Execute code snippets in sandbox","timeout_secs":120,"headers":{"X-Api-Key":"your-api-key"}}}}',
        TRUE,
        FALSE
    );

-- Step 2：将两个 A2A 服务绑定到 agent_id=1
INSERT INTO
    t_agent_a2a_service_rel (id, agent_id, registry_id, enabled)
VALUES
    (1, 1, 1, TRUE),
    (2, 1, 2, TRUE);

-- =============================================================================
-- 可选：命令（Command）注册与绑定
-- 表：
--   t_agent_command_registry —— 命令注册表（per-user）
--     字段：user_id / name / description / content (Markdown 提示词正文)
--          / enabled / deleted；(user_id, name, deleted) 联合唯一
--   t_agent_command_rel —— 智能体↔命令 绑定关系（注意：无 user_id 字段）
--     字段：agent_id / command_id / priority (越小越靠前) / enabled
-- 不需要 Command 时，可以删除下面 4 条 INSERT。
-- =============================================================================
INSERT INTO
    t_agent_command_registry (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        content,
        enabled,
        deleted
    )
VALUES
    (
        1,
        1,
        'review-code',
        '代码评审命令：对粘贴的代码进行风格 / 性能 / 安全审查。',
        '# /review-code\n\n请对随后给出的代码进行评审，从以下三个维度输出意见：\n1. 代码风格与可读性\n2. 潜在的性能问题\n3. 安全风险与边界条件\n\n输出格式使用 Markdown 列表。',
        TRUE,
        FALSE
    );

INSERT INTO
    t_agent_command_registry (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        content,
        enabled,
        deleted
    )
VALUES
    (
        2,
        1,
        'gen-tests',
        '测试生成命令：根据给定函数生成单元测试。',
        '# /gen-tests\n\n请为随后给出的函数生成 Rust 单元测试，要求：\n- 至少覆盖 1 个正向用例 + 1 个边界用例 + 1 个错误用例\n- 使用 #[tokio::test] 处理异步函数\n- 用 assert_eq! / assert! 断言',
        TRUE,
        FALSE
    );

-- 命令绑定：priority 越小越靠前
INSERT INTO
    t_agent_command_rel (id, agent_id, command_id, priority, enabled)
VALUES
    (1, 1, 1, 0, TRUE),
    (2, 1, 2, 1, TRUE);

-- =============================================================================
-- 可选：技能包（Skill）注册与绑定
-- 表：
--   t_agent_skill_package —— 技能安装包，存放完整 zip 二进制。
--     必填：user_id (0=系统级)、skill_code、content_zip (BLOB)、content_sha256
--     status: 0=Installed, 1=Disabled, 2=Broken
--     source_type: 0=builtin, 1=agent_installed, 2=user_manual
--   t_agent_skill_rel —— 智能体↔技能 绑定关系
--     字段：user_id / agent_id / skill_package_id（外键 → skill_package.id）
--          / enabled
--
-- ⚠️ 注意：content_zip 是 BLOB，无法用纯文本 INSERT 给出真实 zip。
--   建议改用以下任一方式：
--     - 调用 service-hub 的技能上传接口（推荐）
--     - MySQL：    INSERT ... VALUES (..., LOAD_FILE('/abs/path/skill.zip'), ...)
--     - PostgreSQL: 使用 \lo_import 或客户端绑定 bytea 参数
--     - SQLite：    使用 readfile('/abs/path/skill.zip')
--
-- 下面给出 SQLite 风格的占位写法，演示字段填充顺序；其它引擎请替换 BLOB 字面量。
-- 不需要 Skill 时，可以删除下面 2 条 INSERT。
-- =============================================================================
INSERT INTO
    t_agent_skill_package (
        id,
        user_id,
        skill_code,
        NAME,
        DESCRIPTION,
        VERSION,
        source_type,
        source_url,
        content_zip,
        content_sha256,
        size_bytes,
        installed_by,
        STATUS
    )
VALUES
    (
        1,
        0,
        'pdf-helper',
        'PDF 助手',
        '提供 PDF 解析与摘要相关能力。',
        '0.1.0',
        0,
        'builtin://pdf-helper',
        -- 占位：真实部署时请替换为 readfile('...zip') / LOAD_FILE('...zip')
        '504B0506000000000000000000000000000000000000',
        '0000000000000000000000000000000000000000000000000000000000000000',
        22,
        0,
        0
    );

INSERT INTO
    t_agent_skill_rel (id, user_id, agent_id, skill_package_id, enabled)
VALUES
    (1, 1, 1, 1, TRUE);

-- =============================================================================
-- 可选：子智能体（Sub-Agent）绑定
-- 表：
--   t_agent_sub_agent_rel
--     字段：agent_id / sub_agent_id / alias / share_context / enabled
--
-- 子智能体本身仍是 t_agent_config 中的一条记录，
-- 因此先插入 agent_id=2 的子 agent，并复用 model_id=1 完成模型绑定。
-- 不需要 Sub-Agent 时，可以删除下面 3 条 INSERT。
-- =============================================================================
-- 1) 子智能体本体
INSERT INTO
    t_agent_config (
        id,
        user_id,
        NAME,
        DESCRIPTION,
        welcome_message,
        system_prompt,
        STATUS,
        tags_json,
        max_delegation_depth,
        deleted
    )
VALUES
    (
        2,
        1,
        'translator-sub-agent',
        '专注中英互译的子智能体',
        NULL,
        '你是一名中英双语翻译专家，仅返回译文，不要附加解释。',
        1,
        '[]',
        3,
        FALSE
    );

-- 2) 给子智能体绑定模型（复用 model_id=1）
INSERT INTO
    t_agent_llm_model_rel (
        id,
        agent_id,
        model_id,
        temperature,
        max_tokens,
        enabled
    )
VALUES
    (2, 2, 1, 0.2, 1024, TRUE);

-- 3) 主 agent_id=1 委派给 sub_agent_id=2，alias=translator
INSERT INTO
    t_agent_sub_agent_rel (
        id,
        agent_id,
        sub_agent_id,
        alias,
        share_context,
        enabled
    )
VALUES
    (1, 1, 2, 'translator', FALSE, TRUE);

-- =============================================================================
-- 可选：工具
-- t_agent_tool_registry: 注册工具, 已经内置
-- =============================================================================
-- INSERT INTO
--     public.t_agent_tool_registry (
--         id,
--         tool_code,
--         "name",
--         DESCRIPTION,
--         schema_json,
--         enabled,
--         created_at,
--         updated_at
--     )
-- VALUES
--     (
--         2,
--         'read_file',
--         '文件读取工具',
--         '读取指定路径的文件内容。支持按行范围读取。',
--         NULL,
--         TRUE,
--         '2026-05-19 03:27:38.078',
--         '2026-05-19 03:27:38.078'
--     );
INSERT INTO
    public.t_agent_tool_rel (
        id,
        agent_id,
        tool_id,
        enabled,
        priority,
        created_at,
        updated_at
    )
VALUES
    (
        5,
        1,
        3,
        TRUE,
        0,
        '2026-05-19 11:47:05.150',
        '2026-05-19 11:47:05.150'
    );
