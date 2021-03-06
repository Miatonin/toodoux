use std::error::Error;

use colored::Colorize;

use crate::{
  cli::{add_task, edit_task, list_tasks, SubCommand},
  config::Config,
  task::{Status, TaskManager, UID},
};

pub fn run_subcmd(
  config: Config,
  subcmd: Option<SubCommand>,
  task_uid: Option<UID>,
) -> Result<(), Box<dyn Error>> {
  match subcmd {
    // default subcommand
    None => {
      default_list(&config, true, true, false, false, false)?;
    }

    Some(subcmd) => {
      let mut task_mgr = TaskManager::new_from_config(&config)?;
      let task = task_uid.and_then(|uid| task_mgr.get_mut(uid));

      match subcmd {
        SubCommand::Add {
          start,
          done,
          content,
        } => {
          if task_uid.is_none() {
            add_task(&config, start, done, content)?;
          } else {
            println!(
              "{}",
              "cannot add a task to another one; maybe you were looking for dependencies instead?"
                .red()
            );
          }
        }

        SubCommand::Edit { content } => {
          if let Some(task) = task {
            edit_task(task, content)?;
            task_mgr.save(&config)?;
          } else {
            println!("{}", "missing or unknown task to edit".red());
          }
        }

        SubCommand::Todo => {
          if let Some(task) = task {
            task.change_status(Status::Todo);
            task_mgr.save(&config)?;
          } else {
            println!("{}", "missing or unknown task".red());
          }
        }

        SubCommand::Start => {
          if let Some(task) = task_uid.and_then(|uid| task_mgr.get_mut(uid)) {
            task.change_status(Status::Ongoing);
            task_mgr.save(&config)?;
          } else {
            println!("{}", "missing or unknown task to start".red());
          }
        }

        SubCommand::Done => {
          if let Some(task) = task_uid.and_then(|uid| task_mgr.get_mut(uid)) {
            task.change_status(Status::Done);
            task_mgr.save(&config)?;
          } else {
            println!("{}", "missing or unknown task to finish".red());
          }
        }

        SubCommand::Cancel => {
          if let Some(task) = task_uid.and_then(|uid| task_mgr.get_mut(uid)) {
            task.change_status(Status::Cancelled);
            task_mgr.save(&config)?;
          } else {
            println!("{}", "missing or unknown task to cancel".red());
          }
        }

        SubCommand::Remove { .. } => {}

        SubCommand::List {
          todo,
          start,
          done,
          cancelled,
          all,
          ..
        } => {
          default_list(&config, todo, start, cancelled, done, all)?;
        }
      }
    }
  }

  Ok(())
}

fn default_list(
  config: &Config,
  mut todo: bool,
  mut start: bool,
  mut cancelled: bool,
  mut done: bool,
  all: bool,
) -> Result<(), Box<dyn Error>> {
  // handle filtering logic
  if all {
    todo = true;
    start = true;
    done = true;
    cancelled = true;
  } else if !(todo || start || done || cancelled) {
    // if nothing is set, we use “sensible” defaults by listing only “active” tasks (todo and ongoing)
    todo = true;
    start = true;
  }

  list_tasks(config, todo, start, cancelled, done)
}
