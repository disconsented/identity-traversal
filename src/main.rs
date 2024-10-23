use crate::hostmask::{Host, HostMask, Ident, Nick, Query};
use clap::Parser;
use futures_util::{stream, StreamExt};
use log::{debug, info};
use ratatui::crossterm::event;
use ratatui::crossterm::event::{poll, KeyCode, KeyEventKind};
use ratatui::prelude::{Color, Constraint, Style, Stylize};
use ratatui::widgets::{Block, Row as TableRow, Table, TableState};
use ratatui::DefaultTerminal;
use std::collections::HashSet;
use std::io;
use std::str::FromStr;
use std::time::Duration;
use itertools::Itertools;
use tokio::sync::oneshot::error::TryRecvError;
use tokio::sync::oneshot::Receiver;
use tokio::time::sleep;
use tokio_postgres::{Client, NoTls, Row};
use tui_logger::{TuiLoggerLevelOutput, TuiLoggerWidget};

mod hostmask;
mod postgres;

fn run(mut terminal: DefaultTerminal, mut rx: Receiver<HashSet<Sender>>) -> io::Result<()> {
    let mut senders: Option<HashSet<Sender>> = None;
    let mut table_state = TableState::default();
    loop {
        terminal.draw(|frame| {
            if let Some(senders) = &mut senders {
                let rows = senders
                    .iter()
                    .sorted_unstable_by(|a, b| Ord::cmp(&a.sender.host(), &b.sender.host()))
                    .map(|sender| {
                        TableRow::new([
                            sender.sender.nick().to_string(),
                            sender.sender.ident().to_string(),
                            sender.sender.host().to_string(),
                        ])
                    })
                    .collect::<Vec<_>>();
                // let rows = [TableRow::new(vec!["Cell1", "Cell2", "Cell3"])];
                // Columns widths are constrained in the same way as Layout...
                let widths = [
                    Constraint::Fill(1),
                    Constraint::Fill(1),
                    Constraint::Fill(1),
                ];
                let table = Table::new(rows, widths)
                    // ...and they can be separated by a fixed spacing.
                    .column_spacing(1)
                    // You can set the style of the entire Table.
                    .style(Style::new().blue())
                    // It has an optional header, which is simply a Row always visible at the top.
                    .header(
                        TableRow::new(vec!["Nick", "Ident", "Host"])
                            .style(Style::new().bold())
                            // To add space between the header and the rest of the rows, specify the margin
                            .bottom_margin(1),
                    )
                    // // It has an optional footer, which is simply a Row always visible at the bottom.
                    // .footer(TableRow::new(vec!["Updated on Dec 28"]))
                    // As any other widget, a Table can be wrapped in a Block.
                    .block(Block::new().title(format!("{} query results", senders.len())))
                    // The selected row and its content can also be styled.
                    .highlight_style(Style::new().reversed())
                    // ...and potentially show a symbol in front of the selection.
                    .highlight_symbol(">>");
                frame.render_stateful_widget(table, frame.area(), &mut table_state);
            } else {
                let logger = TuiLoggerWidget::default()
                    .block(Block::bordered().title("Logs"))
                    .output_separator('|')
                    .output_timestamp(Some("%F %H:%M:%S%.3f".to_string()))
                    .output_level(Some(TuiLoggerLevelOutput::Long))
                    .output_target(false)
                    .output_file(false)
                    .output_line(false)
                    .style(Style::default().fg(Color::White));

                frame.render_widget(logger, frame.area());
            }
        })?;
        if poll(Duration::from_millis(100))? {
            if let event::Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Up => {
                            table_state.select(Some(
                                table_state
                                    .selected()
                                    .map(|index| index.saturating_sub(1))
                                    .unwrap_or(0)
                                    .max(0),
                            ));
                        }
                        KeyCode::Down => {
                            table_state.select(Some(
                                table_state
                                    .selected()
                                    .map(|i| i.saturating_add(1))
                                    .unwrap_or(0)
                                    .min(
                                        senders.as_ref().map(|senders| senders.len()).unwrap_or(0),
                                    ),
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }

        if senders.is_none() {
            match rx.try_recv() {
                Ok(res) => {
                    let _ = event::read()?;
                    senders = Some(res);
                }
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Closed) => panic!("rx closed"),
            }
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// libera style host mask NICK!IDENT@HOST
    mask: String,
    /// for hosts that are also an ip address, do we search the subnet?
    #[clap(short, long)]
    subnet: bool,
    /// how many iterations to traverse
    #[clap(short, long, default_value = "3")]
    depth: usize,
    /// whether to follow idents,
    #[clap(short, long)]
    ident: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tui_logger::init_logger(log::LevelFilter::Trace).unwrap();
    tui_logger::set_default_level(log::LevelFilter::Trace);
    let (tx, rx) = tokio::sync::oneshot::channel::<_>();
    let args = Args::parse();
    debug!("args: {args:?}");
    dbg!(&args);
    run_worker(tx, args);
    let mut terminal = ratatui::init();
    terminal.clear()?;
    let app_result = run(terminal, rx);
    ratatui::restore();
    Ok(app_result?)
}

fn run_worker(tx: tokio::sync::oneshot::Sender<HashSet<Sender>>, args: Args) {
    tokio::task::spawn(async move {
        info!("Hello, world!");
        let (client, connection) =
            tokio_postgres::connect("host=localhost user=quassel dbname=quassel", NoTls)
                .await
                .unwrap();
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });

        // let mask =
        // HostMask::from_str("TeXNickAL!~synick@c-69-138-250-10.hsd1.md.comcast.net").unwrap();
        // let mask = HostMask::from_str("BarlowRidge!~LockHimUp@user/Star2021").unwrap();
        // let mask = HostMask::from_str("roran!~roran@user/roran").unwrap();
        // let mask = HostMask::from_str("Felenov!~Felenov@miraheze/Felenov").unwrap();
        // let mask = HostMask::from_str("Unit640!~Unit640@user/Unit640").unwrap();
        // let mask = HostMask::from_str("kks!~kks@user/kks").unwrap();
        let mut mask = HostMask::from_str(&args.mask).unwrap();
        mask.subnet = args.subnet;

        info!("parsed {mask:?}");
        let depth = 2;
        let mut nicks: HashSet<Nick> = HashSet::from([mask.nick().to_owned()]);
        let mut idents: HashSet<Ident> = HashSet::from([mask.ident().to_owned()]);
        if !args.ident {
            idents.clear();
        }
        let mut hosts: HashSet<Host> = HashSet::from([mask.host().to_owned()]);
        let mut senders: HashSet<Sender> = HashSet::new();
        for i in 0..depth {
            if [nicks.is_empty(), idents.is_empty(), hosts.is_empty()]
                .iter()
                .all(|empty| *empty)
            {
                info!("no new query terms found; ending");
                break;
            }
            debug!(
                "starting iteration {i}; {},{},{}",
                nicks.len(),
                idents.len(),
                hosts.len()
            );
            let mut res = stream::iter(nicks.into_iter())
                .filter_map(|query| async { search(&client, Box::new(query)).await.ok() })
                .map(|rows| {
                    rows.iter()
                        .map(Sender::try_from)
                        .filter_map(|row| {
                            row.ok()
                                .and_then(|mut sender| {
                                    sender.sender.subnet = args.subnet;
                                    senders.insert(sender.clone()).then_some(sender)
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .inspect(|nicks| debug!("found {} nicks", nicks.len()))
                .collect::<Vec<Vec<_>>>()
                .await
                .concat()
                .into_iter()
                .collect::<HashSet<_>>();
            debug!("nicks found: {res:?}");

            if args.ident {
                let r = stream::iter(idents.into_iter())
                    .filter_map(|query| async {
                        // shadowing here forces a move, meaning we don't need to have a move closure
                        debug!("querying for ident {query:?}");
                        search(&client, Box::new(query)).await.ok()
                    })
                    .map(|rows| {
                        rows.iter()
                            .map(Sender::try_from)
                            .filter_map(|row| {
                                row.ok()
                                    .and_then(|mut sender| {
                                        sender.sender.subnet = args.subnet;
                                        senders.insert(sender.clone()).then_some(sender)
                                    })
                            })
                            .collect::<Vec<_>>()
                    })
                    .inspect(|idents| debug!("found {} idents", idents.len()))
                    .collect::<Vec<Vec<_>>>()
                    .await
                    .concat()
                    .into_iter()
                    .collect::<HashSet<_>>();
                debug!("idents found: {r:?}");
                res.extend(r);
            }

            let r = stream::iter(hosts.into_iter())
                .filter_map(|query| async {
                    // shadowing here forces a move, meaning we don't need to have a move closure
                    debug!("querying for host {query:?}");
                    search(&client, Box::new(query)).await.ok()
                })
                .map(|rows| {
                    rows.iter()
                        .map(Sender::try_from)
                        .filter_map(|row| {
                            row.ok()
                                .and_then(|mut sender| {
                                    sender.sender.subnet = args.subnet;
                                    senders.insert(sender.clone()).then_some(sender)
                                })
                        })
                        .collect::<Vec<_>>()
                })
                .inspect(|hosts| debug!("found {} hosts", hosts.len()))
                .collect::<Vec<Vec<_>>>()
                .await
                .concat()
                .into_iter()
                .collect::<HashSet<_>>();
            debug!("hosts found: {r:?}");
            res.extend(r);

            nicks = HashSet::new();
            idents = HashSet::new();
            hosts = HashSet::new();
            res.into_iter().for_each(|sender| {
                nicks.insert(sender.sender.nick().clone());
                idents.insert(sender.sender.ident().clone());
                hosts.insert(sender.sender.host().clone());
            });
            debug!("there are {} total senders", senders.len());
        }
        info!("done; press the any key to continue");
        sleep(Duration::from_secs(1)).await;
        let _ = tx.send(senders);
    });
}

async fn search(
    client: &Client,
    query: Box<dyn Query + Send>,
) -> Result<Vec<Row>, tokio_postgres::Error> {
    info!("query: {}", query.query());
    client
        .query(
            "SELECT senderid, sender, realname FROM sender WHERE sender LIKE $1::TEXT",
            &[&query.query()],
        )
        .await
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
struct Sender {
    id: i64,
    sender: HostMask,
    realname: Option<String>,
}

impl TryFrom<&Row> for Sender {
    type Error = Box<dyn std::error::Error>;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.try_get(0)?,
            sender: HostMask::from_str(row.try_get(1)?)?,
            realname: None,
        })
    }
}
