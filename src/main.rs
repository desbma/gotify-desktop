//! Gotify desktop daemon

use std::{cell::RefCell, process::Command, rc::Rc};

use anyhow::Context as _;

mod config;
mod gotify;
mod notif;

/// Run configured command on message reception
fn run_on_msg_command(
    message: &gotify::Message,
    on_msg_command: &(String, Vec<String>),
) -> anyhow::Result<()> {
    log::info!(
        "Running on message command: {} {}",
        on_msg_command.0,
        on_msg_command.1.join(" ")
    );
    Command::new(&on_msg_command.0)
        .args(&on_msg_command.1)
        .env("GOTIFY_MSG_PRIORITY", format!("{}", message.priority))
        .env("GOTIFY_MSG_TITLE", &message.title)
        .env("GOTIFY_MSG_TEXT", &message.text)
        .status()?;
    //.exit_ok()?;

    Ok(())
}

/// Process new message
fn handle_message(
    message: &gotify::Message,
    min_priority: i64,
    on_msg_command: Option<&(String, Vec<String>)>,
    delete: bool,
    client: &mut gotify::Client,
) -> anyhow::Result<()> {
    log::info!("Got {message:?}");

    if message.priority >= min_priority {
        notif::show(message)?;
    } else {
        log::debug!(
            "Ignoring notification for message of priority {}",
            message.priority
        );
    }

    if let Some(on_msg_command) = on_msg_command {
        if let Err(e) = run_on_msg_command(message, on_msg_command) {
            log::warn!("Command {on_msg_command:?} failed with error: {e:?}");
        }
    }

    if delete {
        client.delete_message(message.id)?;
    }

    Ok(())
}

/// Program entry point
fn main() -> anyhow::Result<()> {
    // Init logger
    simple_logger::SimpleLogger::new()
        .init()
        .context("Failed to init logger")?;

    // Parse config
    let cfg = config::parse().context("Failed to read config")?;
    let token = cfg.gotify.token.fetch()?;
    let on_msg_command = match cfg.action.on_msg_command {
        None => None,
        Some(cmd) => Some(
            shlex::split(&cmd)
                .with_context(|| format!("Failed to split command arguments for {cmd:?}"))?
                .split_first()
                .map(|t| (t.0.to_owned(), t.1.to_owned()))
                .ok_or_else(|| anyhow::anyhow!("Empty command"))?,
        ),
    };

    // Keep last handled message id
    let last_msg_id = Rc::new(RefCell::new(None));

    // Connect loop
    loop {
        // Connect
        let mut client = gotify::Client::connect(&cfg.gotify, &token, Rc::clone(&last_msg_id))
            .context("Failed to setup or connect client")?;
        log::info!("Connected to {}", cfg.gotify.url);

        // Handle missed messages
        let missed_messages = client
            .get_missed_messages()
            .context("Failed to get missed messages")?;
        if !missed_messages.is_empty() {
            log::info!("Catching up {} missed message(s)", missed_messages.len());
            for msg in missed_messages {
                handle_message(
                    &msg,
                    cfg.notification.min_priority,
                    on_msg_command.as_ref(),
                    cfg.gotify.auto_delete,
                    &mut client,
                )
                .context("Failed to handle message")?;
            }
        }

        // Blocking message loop
        loop {
            let res = client.get_message();
            let msg = match res {
                Ok(m) => m,
                #[expect(clippy::ref_patterns)]
                Err(ref e) => {
                    if e.downcast_ref::<gotify::NeedsReconnect>().is_some() {
                        log::warn!("Error while waiting for message: {e}, will try to reconnect");
                        break;
                    }
                    res.context("Failed to get message")?;
                    unreachable!();
                }
            };

            handle_message(
                &msg,
                cfg.notification.min_priority,
                on_msg_command.as_ref(),
                cfg.gotify.auto_delete,
                &mut client,
            )
            .context("Failed to handle message")?;
        }
    }
}
