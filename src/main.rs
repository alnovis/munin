mod render;

use std::env;
use std::error::Error as StdError;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;

use munin::time::now_ms;
use munin::{
    Kind, ListQuery, NewEntry, Scope, ScopeKind, SearchQuery, Store, config, mcp, project, protocol,
};

type CliResult<T> = std::result::Result<T, Box<dyn StdError>>;

#[derive(Parser)]
#[command(name = "munin", version, about = "Local memory for AI coding agents")]
struct Cli {
    /// Database file [env: MUNIN_DB] [default: ~/.munin/munin.db]
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Print JSON instead of text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search memory; words are OR-ed, a "quoted phrase" matches in order
    Search {
        #[arg(required = true)]
        query: Vec<String>,
        #[command(flatten)]
        scope: ScopeArgs,
        /// Current project [env: MUNIN_PROJECT] [default: git root or directory name]
        #[arg(long)]
        project: Option<String>,
        /// Include superseded and retracted entries
        #[arg(long)]
        inactive: bool,
        /// Rank on text alone, ignoring age
        #[arg(long)]
        deep: bool,
        /// Number of cards, 1 to 15 [default: 5]
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Show one entry with its chain and events
    Show { id: i64 },
    /// Record a new entry
    Add {
        /// decision, architecture, bugfix or note
        #[arg(long)]
        kind: Kind,
        /// One line, at most 200 characters
        #[arg(long)]
        summary: String,
        #[arg(long, conflicts_with = "body_file")]
        body: Option<String>,
        /// Read the body from a file; `-` reads standard input
        #[arg(long)]
        body_file: Option<PathBuf>,
        /// Repeat for several tags, at most 6
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// Id of the entry this one replaces
        #[arg(long)]
        supersedes: Option<i64>,
        /// Current project [env: MUNIN_PROJECT] [default: git root or directory name]
        #[arg(long)]
        project: Option<String>,
        /// Record the entry for this group instead of the project
        #[arg(long)]
        group: Option<String>,
    },
    /// Mark an entry as wrong; it stays readable
    Retract {
        id: i64,
        #[arg(long)]
        reason: String,
    },
    /// List tags in use, most recently used first
    Tags {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Current project [env: MUNIN_PROJECT] [default: git root or directory name]
        #[arg(long)]
        project: Option<String>,
        /// Number of tags, 1 to 200 [default: 40]
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List every entry in scope, newest first, without a search query
    List {
        #[command(flatten)]
        scope: ScopeArgs,
        /// Current project [env: MUNIN_PROJECT] [default: git root or directory name]
        #[arg(long)]
        project: Option<String>,
        /// Only entries of this kind: decision, architecture, bugfix or note
        #[arg(long)]
        kind: Option<Kind>,
        /// Include superseded and retracted entries
        #[arg(long)]
        inactive: bool,
        /// At most this many entries [default: all]
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Summarise what memory holds for the current project, for agent context
    Brief {
        /// Current project [env: MUNIN_PROJECT] [default: git root or directory name]
        #[arg(long)]
        project: Option<String>,
    },
    /// Print the protocol agents follow when using Munin
    Protocol,
    /// Run the MCP server on standard input and output
    Serve,
    /// Manage groups of projects worked on together
    #[command(subcommand)]
    Group(GroupCommand),
}

#[derive(Subcommand)]
enum GroupCommand {
    /// Add projects to a group, creating the group if needed
    Add {
        group: String,
        #[arg(required = true)]
        projects: Vec<String>,
    },
    /// Remove projects from a group
    Remove {
        group: String,
        #[arg(required = true)]
        projects: Vec<String>,
    },
    /// Delete a group; refused while it owns entries
    Delete { group: String },
    /// List all groups, or the groups a project belongs to
    List {
        #[arg(long)]
        project: Option<String>,
    },
}

#[derive(Args)]
struct ScopeArgs {
    /// Where to look [default: group when the project belongs to one, else project]
    #[arg(long, value_enum)]
    scope: Option<ScopeArg>,
    /// Look in this group and its projects
    #[arg(long)]
    group: Option<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum ScopeArg {
    Project,
    Group,
    All,
}

impl ScopeArgs {
    fn resolve(self) -> CliResult<Option<Scope>> {
        let kind = self.scope.map(|scope| match scope {
            ScopeArg::Project => ScopeKind::Project,
            ScopeArg::Group => ScopeKind::Group,
            ScopeArg::All => ScopeKind::All,
        });
        Ok(Scope::from_parts(kind, self.group)?)
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("munin: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> CliResult<()> {
    let json = cli.json;
    if let Command::Protocol = cli.command {
        print!("{}", protocol::TEXT);
        return Ok(());
    }
    let mut store = open_store(cli.db)?;

    match cli.command {
        Command::Protocol => unreachable!("handled above"),
        Command::Serve => {
            let server = mcp::Server::new(
                store,
                env::var(config::ENV_PROJECT).ok(),
                env::current_dir()?,
            );
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(mcp::serve(server))
        }
        Command::Search {
            query,
            scope,
            project,
            inactive,
            deep,
            limit,
        } => {
            let scope = scope.resolve()?;
            let cards = store.search(
                &SearchQuery {
                    text: query.join(" "),
                    project: scoped_project(scope.as_ref(), project.as_deref())?,
                    scope,
                    include_inactive: inactive,
                    deep_history: deep,
                    limit,
                },
                now_ms(),
            )?;
            emit(json, &cards[..], render::cards)
        }
        Command::Show { id } => emit(json, &store.get(id)?, render::entry),
        Command::Add {
            kind,
            summary,
            body,
            body_file,
            tags,
            supersedes,
            project,
            group,
        } => {
            let body = match (body, body_file) {
                (Some(body), _) => body,
                (None, Some(path)) => read_body(&path)?,
                (None, None) => String::new(),
            };
            let id = store.append(
                &NewEntry {
                    project: current_project(project.as_deref())?,
                    group,
                    kind,
                    summary,
                    body,
                    tags,
                    supersedes,
                },
                now_ms(),
            )?;
            emit(json, &store.get(id)?, |e| format!("added #{}\n", e.id))
        }
        Command::Retract { id, reason } => {
            store.retract(id, &reason, now_ms())?;
            emit(json, &store.get(id)?, |e| format!("retracted #{}\n", e.id))
        }
        Command::Tags {
            scope,
            project,
            limit,
        } => {
            let scope = scope.resolve()?;
            let project = scoped_project(scope.as_ref(), project.as_deref())?;
            let tags = store.tags(project.as_deref(), scope.as_ref(), limit, now_ms())?;
            emit(json, &tags[..], render::tags)
        }
        Command::List {
            scope,
            project,
            kind,
            inactive,
            limit,
        } => {
            let scope = scope.resolve()?;
            let listed = store.list(
                &ListQuery {
                    project: scoped_project(scope.as_ref(), project.as_deref())?,
                    scope,
                    kind,
                    include_inactive: inactive,
                    limit,
                },
                now_ms(),
            )?;
            emit(json, &listed[..], render::listed)
        }
        Command::Brief { project } => {
            let brief = store.brief(&current_project(project.as_deref())?, now_ms())?;
            emit(json, &brief, render::brief)
        }
        Command::Group(command) => run_group(&mut store, command, json),
    }
}

fn run_group(store: &mut Store, command: GroupCommand, json: bool) -> CliResult<()> {
    let shown = match command {
        GroupCommand::Add { group, projects } => {
            store.add_members(&group, &projects)?;
            group
        }
        GroupCommand::Remove { group, projects } => {
            store.remove_members(&group, &projects)?;
            group
        }
        GroupCommand::Delete { group } => {
            store.delete_group(&group)?;
            if json {
                println!("{}", serde_json::json!({ "deleted": group }));
            } else {
                println!("deleted group {group}");
            }
            return Ok(());
        }
        GroupCommand::List { project } => {
            let groups = store.groups(project.as_deref())?;
            return emit(json, &groups[..], render::groups);
        }
    };
    let groups: Vec<_> = store
        .groups(None)?
        .into_iter()
        .filter(|g| g.name.eq_ignore_ascii_case(shown.trim()))
        .collect();
    emit(json, &groups[..], render::groups)
}

fn open_store(db: Option<PathBuf>) -> CliResult<Store> {
    let path = match db {
        Some(path) => path,
        None => config::db_path(env::var_os(config::ENV_DB), env::var_os("HOME"))?,
    };
    let options = config::options(env::var(config::ENV_HALF_LIFE_DAYS).ok())?;
    Ok(Store::open_with(&path, options)?)
}

// Scope `all` does not depend on the project, so without an explicit one it
// must not fail in a directory that names no project, such as `/`.
fn scoped_project(scope: Option<&Scope>, explicit: Option<&str>) -> CliResult<Option<String>> {
    if matches!(scope, Some(Scope::All)) && explicit.is_none() {
        return Ok(None);
    }
    Ok(Some(current_project(explicit)?))
}

fn current_project(explicit: Option<&str>) -> CliResult<String> {
    let from_env = env::var(config::ENV_PROJECT).ok();
    Ok(project::resolve(
        explicit,
        from_env.as_deref(),
        &env::current_dir()?,
    )?)
}

fn read_body(path: &Path) -> CliResult<String> {
    if path == Path::new("-") {
        let mut body = String::new();
        std::io::stdin().read_to_string(&mut body)?;
        Ok(body)
    } else {
        std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()).into())
    }
}

fn emit<T: Serialize + ?Sized>(
    json: bool,
    value: &T,
    text: impl Fn(&T) -> String,
) -> CliResult<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        print!("{}", text(value));
    }
    Ok(())
}
