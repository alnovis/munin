use std::fmt::Display;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::stdio;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::time::now_ms;
use crate::{Kind, NewEntry, Scope, ScopeKind, SearchQuery, Store, project, protocol};

type ToolResult = Result<String, String>;

/// The MCP server: one store shared by all tool calls of a session.
#[derive(Clone)]
pub struct Server {
    store: Arc<Mutex<Store>>,
    env_project: Option<String>,
    dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum KindParam {
    Decision,
    Architecture,
    Bugfix,
    Note,
}

impl From<KindParam> for Kind {
    fn from(kind: KindParam) -> Self {
        match kind {
            KindParam::Decision => Kind::Decision,
            KindParam::Architecture => Kind::Architecture,
            KindParam::Bugfix => Kind::Bugfix,
            KindParam::Note => Kind::Note,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ScopeParam {
    Project,
    Group,
    All,
}

impl From<ScopeParam> for ScopeKind {
    fn from(scope: ScopeParam) -> Self {
        match scope {
            ScopeParam::Project => ScopeKind::Project,
            ScopeParam::Group => ScopeKind::Group,
            ScopeParam::All => ScopeKind::All,
        }
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Words to look for, OR-ed; wrap a phrase in double quotes.
    pub query: String,
    /// project: this project only. group: this project, every project that
    /// shares a group with it, and those groups. all: everything. Default:
    /// group when the project belongs to a group, project otherwise.
    pub scope: Option<ScopeParam>,
    /// Look in this group and its projects.
    pub group: Option<String>,
    /// Override the current project.
    pub project: Option<String>,
    /// Also return superseded and retracted entries.
    #[serde(default)]
    pub include_inactive: bool,
    /// Rank on text alone, ignoring age.
    #[serde(default)]
    pub deep_history: bool,
    /// Number of cards, 1 to 15; default 5.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EntryParams {
    /// Entry id from a search card.
    pub id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AppendParams {
    /// decision and architecture never fade; bugfix and note fade over about a month.
    pub kind: KindParam,
    /// One line, at most 200 characters.
    pub summary: String,
    /// Context, what was tried, why this choice.
    #[serde(default)]
    pub body: String,
    /// At most 6; reuse tags from get_active_tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Id of the active entry this one replaces.
    pub supersedes: Option<i64>,
    /// Override the current project.
    pub project: Option<String>,
    /// Record the entry for this group instead of the project; the project
    /// must belong to it.
    pub group: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RetractParams {
    /// Id of the active entry that is wrong.
    pub id: i64,
    /// Why the entry is wrong.
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TagsParams {
    /// Same meaning as in search_memory.
    pub scope: Option<ScopeParam>,
    /// Same meaning as in search_memory.
    pub group: Option<String>,
    /// Override the current project.
    pub project: Option<String>,
    /// Number of tags, 1 to 200; default 40.
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GroupsParams {
    /// Override the current project.
    pub project: Option<String>,
}

#[tool_router(router = tool_router)]
impl Server {
    /// `env_project` is `MUNIN_PROJECT`; `dir` is where the client started the
    /// server, normally the project the agent works in.
    pub fn new(store: Store, env_project: Option<String>, dir: PathBuf) -> Self {
        Server {
            store: Arc::new(Mutex::new(store)),
            env_project,
            dir,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Search memory before unfamiliar work. Returns compact cards; read a full entry with get_entry."
    )]
    async fn search_memory(&self, Parameters(p): Parameters<SearchParams>) -> ToolResult {
        let scope = Scope::from_parts(p.scope.map(Into::into), p.group).map_err(message)?;
        let query = SearchQuery {
            text: p.query,
            project: self.scoped_project(scope.as_ref(), p.project.as_deref())?,
            scope,
            include_inactive: p.include_inactive,
            deep_history: p.deep_history,
            limit: p.limit,
        };
        to_json(&self.store()?.search(&query, now_ms()).map_err(message)?)
    }

    #[tool(description = "Read one entry in full, with its supersede chain and events.")]
    async fn get_entry(&self, Parameters(p): Parameters<EntryParams>) -> ToolResult {
        to_json(&self.store()?.get(p.id).map_err(message)?)
    }

    #[tool(
        description = "Record a decision, architecture fact, bug fix or note after finishing work. Use supersedes to replace an outdated entry."
    )]
    async fn append_memory(&self, Parameters(p): Parameters<AppendParams>) -> ToolResult {
        let new = NewEntry {
            project: self.project(p.project.as_deref())?,
            group: p.group,
            kind: p.kind.into(),
            summary: p.summary,
            body: p.body,
            tags: p.tags,
            supersedes: p.supersedes,
        };
        let mut store = self.store()?;
        let id = store.append(&new, now_ms()).map_err(message)?;
        to_json(&store.get(id).map_err(message)?)
    }

    #[tool(description = "Mark an entry as wrong, with the reason. The entry stays readable.")]
    async fn retract_memory(&self, Parameters(p): Parameters<RetractParams>) -> ToolResult {
        let mut store = self.store()?;
        store.retract(p.id, &p.reason, now_ms()).map_err(message)?;
        to_json(&store.get(p.id).map_err(message)?)
    }

    #[tool(
        description = "List tags in use, most recently used first. Call before append_memory and reuse existing tags."
    )]
    async fn get_active_tags(&self, Parameters(p): Parameters<TagsParams>) -> ToolResult {
        let scope = Scope::from_parts(p.scope.map(Into::into), p.group).map_err(message)?;
        let project = self.scoped_project(scope.as_ref(), p.project.as_deref())?;
        let tags = self
            .store()?
            .tags(project.as_deref(), scope.as_ref(), p.limit, now_ms())
            .map_err(message)?;
        to_json(&tags)
    }

    #[tool(
        description = "List the groups the current project belongs to, with their member projects."
    )]
    async fn list_groups(&self, Parameters(p): Parameters<GroupsParams>) -> ToolResult {
        let project = self.project(p.project.as_deref())?;
        to_json(&self.store()?.groups(Some(&project)).map_err(message)?)
    }
}

impl Server {
    fn project(&self, explicit: Option<&str>) -> Result<String, String> {
        project::resolve(explicit, self.env_project.as_deref(), &self.dir).map_err(message)
    }

    // Scope `all` does not depend on the project; without an explicit one it
    // must work wherever the client started the server.
    fn scoped_project(
        &self,
        scope: Option<&Scope>,
        explicit: Option<&str>,
    ) -> Result<Option<String>, String> {
        if matches!(scope, Some(Scope::All)) && explicit.is_none() {
            return Ok(None);
        }
        self.project(explicit).map(Some)
    }

    fn store(&self) -> Result<MutexGuard<'_, Store>, String> {
        self.store
            .lock()
            .map_err(|_| "memory store is unavailable after an earlier failure".to_owned())
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("munin", env!("CARGO_PKG_VERSION")))
            .with_instructions(protocol::TEXT)
    }
}

/// Serves `server` over stdin and stdout until the client disconnects.
pub async fn serve(server: Server) -> Result<(), Box<dyn std::error::Error>> {
    let running = server.serve(stdio()).await?;
    running.waiting().await?;
    Ok(())
}

fn message(err: impl Display) -> String {
    err.to_string()
}

fn to_json(value: &impl Serialize) -> ToolResult {
    serde_json::to_string(value).map_err(message)
}
