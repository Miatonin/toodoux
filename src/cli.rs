//! Command line interface.

use chrono::{Duration, Utc};
use colored::Colorize;
use std::{error::Error, fmt::Display, iter::once, path::PathBuf};
use structopt::StructOpt;

use crate::{
  config::Config, metadata::Metadata, metadata::Priority, task::Status, task::Task,
  task::TaskManager, task::UID,
};

#[derive(Debug, StructOpt)]
#[structopt(
  name = "toodoux",
  about = "A modern task / todo / note management tool."
)]
pub struct Command {
  /// UID of a task to operate on.
  pub task_uid: Option<UID>,

  #[structopt(subcommand)]
  pub subcmd: Option<SubCommand>,

  /// Non-default config root to read data and configuration from.
  #[structopt(long, short)]
  pub config: Option<PathBuf>,
}

#[derive(Debug, StructOpt)]
pub enum SubCommand {
  /// Add a task.
  #[structopt(visible_aliases = &["a"])]
  Add {
    /// Mark the item as ONGOING.
    #[structopt(long)]
    start: bool,

    /// Mark the item as DONE.
    #[structopt(long)]
    done: bool,

    /// Content of the task.
    ///
    /// If nothing is set, an interactive prompt is spawned for you to enter the content
    /// of what to do.
    content: Vec<String>,
  },

  /// Edit a task.
  #[structopt(visible_aliases = &["e", "ed"])]
  Edit {
    /// Change the name or metadata of the task.
    content: Vec<String>,
  },

  /// Mark a task as todo.
  Todo,

  /// Mark a task as started.
  Start,

  /// Mark a task as done.
  Done,

  /// Mark a task as cancelled.
  Cancel,

  /// Remove a task.
  #[structopt(visible_aliases = &["r", "rm"])]
  Remove {
    /// Remove all the tasks.
    #[structopt(short, long)]
    all: bool,
  },

  /// List all the tasks.
  #[structopt(visible_aliases = &["l", "ls"])]
  List {
    /// Filter with todo items.
    #[structopt(short, long)]
    todo: bool,

    /// Filter with started items.
    #[structopt(short, long)]
    start: bool,

    /// Filter with done items.
    #[structopt(short, long)]
    done: bool,

    /// Filter with cancelled items.
    #[structopt(short, long)]
    cancelled: bool,

    /// Do not filter the items and show them all.
    #[structopt(short, long)]
    all: bool,

    /// Show the content of each listed task, if any.
    #[structopt(long)]
    content: bool,
  },
}

/// List tasks.
pub fn list_tasks(
  config: &Config,
  todo: bool,
  start: bool,
  cancelled: bool,
  done: bool,
) -> Result<(), Box<dyn Error>> {
  let task_mgr = TaskManager::new_from_config(config)?;
  let mut tasks: Vec<_> = task_mgr
    .tasks()
    .filter(|(_, task)| {
      // filter the task depending on what is passed as argument
      match task.status() {
        Status::Ongoing => start,
        Status::Todo => todo,
        Status::Done => done,
        Status::Cancelled => cancelled,
      }
    })
    .collect();
  tasks.sort_by_key(|(_, task)| task.status());

  // precompute a bunch of data for display widths / padding / etc.
  let display_opts = DisplayOptions::new(config, tasks.iter().map(|&(uid, task)| (*uid, task)));

  // actual display
  display_task_header(config, &display_opts);

  let mut parity = true;
  for (&uid, task) in tasks {
    display_task_inline(config, uid, task, parity, &display_opts);

    parity = !parity;
  }

  Ok(())
}

/// Add a new task.
pub fn add_task(
  config: &Config,
  start: bool,
  done: bool,
  content: Vec<String>,
) -> Result<(), Box<dyn Error>> {
  // validate the metadata extracted from the content, if any
  let (metadata, name) = Metadata::from_words(content.iter().map(|s| s.as_str()));
  Metadata::validate(&metadata)?;

  let mut task_mgr = TaskManager::new_from_config(config)?;
  let mut task = Task::new(name, Vec::new());

  // apply the metadata
  task.apply_metadata(metadata);

  // determine if we need to switch to another status
  if start {
    task.change_status(Status::Ongoing);
  } else if done {
    task.change_status(Status::Done);
  }

  let uid = task_mgr.register_task(task.clone());
  task_mgr.save(config)?;

  // display options
  let display_opts = DisplayOptions::new(config, once((uid, &task)));

  display_task_header(config, &display_opts);
  display_task_inline(config, uid, &task, true, &display_opts);

  Ok(())
}

/// Edit a task’s name or metadata.
pub fn edit_task(task: &mut Task, content: Vec<String>) -> Result<(), Box<dyn Error>> {
  // validate the metadata extracted from the content, if any
  let (metadata, name) = Metadata::from_words(content.iter().map(|s| s.as_str()));
  Metadata::validate(&metadata)?;

  // apply the metadata
  task.apply_metadata(metadata);

  // if we have a new name, apply it too
  if !name.is_empty() {
    task.change_name(name);
  }

  Ok(())
}

/// Display options to use when rendering in CLI.
struct DisplayOptions {
  /// Width of the task UID column.
  task_uid_width: usize,
  /// Width of the task status column.
  status_width: usize,
  /// Width of the task description column.
  description_width: usize,
  /// Width of the task project column.
  project_width: usize,
  /// Whether any task has spent time.
  has_spent_time: bool,
  /// Whether we have a priority in at least one task.
  has_priorities: bool,
  /// Whether we have a project in at least one task.
  has_projects: bool,
}

impl DisplayOptions {
  /// Create a new renderer for a set of tasks.
  fn new<'a>(config: &Config, tasks: impl IntoIterator<Item = (UID, &'a Task)>) -> Self {
    let (
      task_uid_width,
      status_width,
      description_width,
      project_width,
      has_spent_time,
      has_priorities,
      has_projects,
    ) = tasks.into_iter().fold(
      (0, 0, 0, 0, false, false, false),
      |(
        task_uid_width,
        status_width,
        description_width,
        project_width,
        has_spent_time,
        has_priorities,
        has_projects,
      ),
       (uid, task)| {
        let task_uid_width = task_uid_width.max(Self::guess_task_uid_width(uid));
        let status_width = status_width.max(Self::guess_task_status_width(&config, task.status()));
        let description_width = description_width.max(task.name().len());
        let project_width = project_width.max(Self::guess_task_project_width(&task).unwrap_or(0));
        let has_spent_tiem = has_spent_time || task.spent_time() != Duration::zero();
        let has_priorities = has_priorities || task.priority().is_some();
        let has_projects = has_projects || task.project().is_some();

        (
          task_uid_width,
          status_width,
          description_width,
          project_width,
          has_spent_time,
          has_priorities,
          has_projects,
        )
      },
    );

    Self {
      task_uid_width: task_uid_width.max(config.uid_col_name().len()),
      status_width: status_width.max(config.status_col_name().len()),
      description_width: description_width.max(config.description_col_name().len()),
      project_width: project_width.max(config.project_col_name().len()),
      has_spent_time,
      has_priorities,
      has_projects,
    }
  }

  /// Guess the width required to represent the task UID.
  fn guess_task_uid_width(uid: UID) -> usize {
    let val = uid.val();

    if val < 10 {
      1
    } else if val < 100 {
      2
    } else if val < 1000 {
      3
    } else if val < 10000 {
      4
    } else if val < 100000 {
      5
    } else {
      6
    }
  }

  /// Guess the width required to represent the task status.
  fn guess_task_status_width(config: &Config, status: Status) -> usize {
    let width = match status {
      Status::Ongoing => config.wip_alias().len(),
      Status::Todo => config.todo_alias().len(),
      Status::Done => config.done_alias().len(),
      Status::Cancelled => config.cancelled_alias().len(),
    };

    width.max("Status".len())
  }

  fn guess_task_project_width(task: &Task) -> Option<usize> {
    task.project().map(str::len)
  }
}

/// Display the header of tasks.
fn display_task_header(config: &Config, opts: &DisplayOptions) {
  print!(
    " {uid:<uid_width$} {age:<age_width$}",
    uid = config.uid_col_name().underline(),
    uid_width = opts.task_uid_width,
    age = config.age_col_name().underline(),
    age_width = config.age_col_name().len(),
  );

  if opts.has_spent_time {
    print!(
      " {spent:<spent_width$}",
      spent = config.spent_col_name().underline(),
      spent_width = config.spent_col_name().len(),
    );
  }

  if opts.has_priorities {
    print!(
      " {priority:<prio_width$}",
      priority = config.prio_col_name().underline(),
      prio_width = config.prio_col_name().len(),
    );
  }

  if opts.has_projects {
    print!(
      " {project:<project_width$}",
      project = config.project_col_name().underline(),
      project_width = opts.project_width,
    );
  }

  println!(
    " {status:<status_width$} {description:<description_width$}",
    status = config.status_col_name().underline(),
    status_width = opts.status_width,
    description = config.description_col_name().underline(),
    description_width = opts.description_width,
  );
}

/// Display a task to the user.
fn display_task_inline(
  config: &Config,
  uid: UID,
  task: &Task,
  parity: bool,
  opts: &DisplayOptions,
) {
  let (name, status);
  let task_status = task.status();

  match task_status {
    Status::Todo => {
      if parity {
        name = task.name().bright_white().on_black();
      } else {
        name = task.name().bright_white().on_bright_black();
      }
      status = config.todo_alias().clone().bold().magenta();
    }

    Status::Ongoing => {
      name = task.name().black().on_bright_green();
      status = config.wip_alias().clone().bold().green();
    }

    Status::Done => {
      name = task.name().bright_black().dimmed().on_black();
      status = config.done_alias().clone().dimmed().bright_black();
    }

    Status::Cancelled => {
      name = task
        .name()
        .bright_black()
        .dimmed()
        .strikethrough()
        .on_black();
      status = config.cancelled_alias().clone().dimmed().bright_red();
    }
  }

  let spent_time = friendly_spent_time(task.spent_time(), task_status);

  print!(
    " {uid:<uid_width$} {age:<age_width$}",
    uid = uid,
    uid_width = opts.task_uid_width,
    age = friendly_task_age(task),
    age_width = config.age_col_name().len(),
  );

  if opts.has_spent_time {
    print!(
      " {spent:<spent_width$}",
      spent = spent_time,
      spent_width = config.spent_col_name().len(),
    );
  }

  if opts.has_priorities {
    print!(
      " {priority:<prio_width$}",
      priority = friendly_priority(task),
      prio_width = config.prio_col_name().len(),
    );
  }

  if opts.has_projects {
    print!(
      " {project:<project_width$}",
      project = friendly_project(task),
      project_width = opts.project_width,
    );
  }

  println!(
    " {status:<status_width$} {name:<name_width$}",
    status = status,
    status_width = opts.status_width,
    name = name,
    name_width = opts.description_width,
  );
}

/// Find out the age of a task and get a friendly representation.
fn friendly_task_age(task: &Task) -> String {
  let dur =
    Utc::now().signed_duration_since(task.creation_date().cloned().unwrap_or_else(|| Utc::now()));
  friendly_duration(dur)
}

pub fn friendly_duration(dur: Duration) -> String {
  if dur.num_minutes() < 1 {
    format!("{}s", dur.num_seconds())
  } else if dur.num_hours() < 1 {
    format!("{}min", dur.num_minutes())
  } else if dur.num_days() < 1 {
    format!("{}h", dur.num_hours())
  } else if dur.num_weeks() < 2 {
    format!("{}d", dur.num_days())
  } else if dur.num_weeks() < 4 {
    // less than four weeks
    format!("{}w", dur.num_weeks())
  } else {
    format!("{}m", dur.num_weeks() / 4)
  }
}

fn friendly_priority(task: &Task) -> impl Display {
  if let Some(prio) = task.priority() {
    match prio {
      Priority::Low => "LOW".bright_black().dimmed(),
      Priority::Medium => "MED".blue(),
      Priority::High => "HIGH".red(),
      Priority::Critical => "CRIT".black().on_bright_red(),
    }
  } else {
    "".normal()
  }
}

fn friendly_project(task: &Task) -> impl Display {
  if let Some(project) = task.project() {
    project.italic()
  } else {
    "".normal()
  }
}

/// String representation of a spent-time.
///
/// If no time has been spent on this task, an empty string is returned.
fn friendly_spent_time(dur: Duration, status: Status) -> impl Display {
  if dur == Duration::zero() {
    return String::new().normal();
  }

  let output = friendly_duration(dur);

  match status {
    Status::Ongoing => output.blue(),
    _ => output.bright_black().dimmed(),
  }
}
