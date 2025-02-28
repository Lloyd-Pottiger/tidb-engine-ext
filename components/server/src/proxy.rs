// Copyright 2016 TiKV Project Authors. Licensed under Apache-2.0.

use std::path::Path;
use std::process;

use crate::setup::{ensure_no_unrecognized_config, validate_and_persist_config};
use clap::{App, Arg};
use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use tikv::config::TiKvConfig;

pub unsafe fn run_proxy(
    argc: c_int,
    argv: *const *const c_char,
    engine_store_server_helper: *const u8,
) {
    raftstore::engine_store_ffi::init_engine_store_server_helper(engine_store_server_helper);
    let engine_store_server_helper = raftstore::engine_store_ffi::gen_engine_store_server_helper(
        engine_store_server_helper as isize,
    );

    let mut args = vec![];

    for i in 0..argc {
        let raw = CStr::from_ptr(*argv.offset(i as isize));
        args.push(raw.to_str().unwrap());
    }

    engine_store_server_helper.check();

    let matches = App::new("RaftStore Proxy")
        .about("RaftStore proxy to connect TiKV cluster")
        .author("tongzhigao@pingcap.com")
        .version(crate::proxy_version_info().as_ref())
        .long_version(crate::proxy_version_info().as_ref())
        .arg(
            Arg::with_name("config")
                .short("C")
                .long("config")
                .value_name("FILE")
                .help("Set the configuration file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("config-check")
                .required(false)
                .long("config-check")
                .takes_value(false)
                .help("Check config file validity and exit"),
        )
        .arg(
            Arg::with_name("log-level")
                .short("L")
                .long("log-level")
                .alias("log")
                .takes_value(true)
                .value_name("LEVEL")
                .possible_values(&[
                    "trace", "debug", "info", "warn", "warning", "error", "critical", "fatal",
                ])
                .help("Set the log level"),
        )
        .arg(
            Arg::with_name("log-file")
                .short("f")
                .long("log-file")
                .takes_value(true)
                .value_name("FILE")
                .help("Sets log file")
                .long_help("Set the log file path. If not set, logs will output to stderr"),
        )
        .arg(
            Arg::with_name("addr")
                .short("A")
                .long("addr")
                .takes_value(true)
                .value_name("IP:PORT")
                .help("Set the listening address"),
        )
        .arg(
            Arg::with_name("advertise-addr")
                .long("advertise-addr")
                .takes_value(true)
                .value_name("IP:PORT")
                .help("Set the advertise listening address for client communication"),
        )
        .arg(
            Arg::with_name("status-addr")
                .long("status-addr")
                .takes_value(true)
                .value_name("IP:PORT")
                .help("Set the HTTP listening address for the status report service"),
        )
        .arg(
            Arg::with_name("advertise-status-addr")
                .long("advertise-status-addr")
                .takes_value(true)
                .value_name("IP:PORT")
                .help("Set the advertise listening address for the client communication of status report service"),
        )
        .arg(
            Arg::with_name("data-dir")
                .long("data-dir")
                .short("s")
                .alias("store")
                .takes_value(true)
                .value_name("PATH")
                .help("Set the directory used to store data"),
        )
        .arg(
            Arg::with_name("capacity")
                .long("capacity")
                .takes_value(true)
                .value_name("CAPACITY")
                .help("Set the store capacity")
                .long_help("Set the store capacity to use. If not set, use entire partition"),
        )
        .arg(
            Arg::with_name("pd-endpoints")
                .long("pd-endpoints")
                .aliases(&["pd", "pd-endpoint"])
                .takes_value(true)
                .value_name("PD_URL")
                .multiple(true)
                .use_delimiter(true)
                .require_delimiter(true)
                .value_delimiter(",")
                .help("Sets PD endpoints")
                .long_help("Set the PD endpoints to use. Use `,` to separate multiple PDs"),
        )
        .arg(
            Arg::with_name("labels")
                .long("labels")
                .alias("label")
                .takes_value(true)
                .value_name("KEY=VALUE")
                .multiple(true)
                .use_delimiter(true)
                .require_delimiter(true)
                .value_delimiter(",")
                .help("Sets server labels")
                .long_help(
                    "Set the server labels. Uses `,` to separate kv pairs, like \
                     `zone=cn,disk=ssd`",
                ),
        )
        .arg(
            Arg::with_name("print-sample-config")
                .long("print-sample-config")
                .help("Print a sample config to stdout"),
        )
        .arg(
            Arg::with_name("metrics-addr")
                .long("metrics-addr")
                .value_name("IP:PORT")
                .hidden(true)
                .help("Sets Prometheus Pushgateway address")
                .long_help(
                    "Sets push address to the Prometheus Pushgateway, \
                     leaves it empty will disable Prometheus push",
                ),
        )
        .arg(
            Arg::with_name("engine-version")
                .long("engine-version")
                .help("Set engine version")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("engine-git-hash")
                .long("engine-git-hash")
                .help("Set engine git hash")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("engine-addr")
                .long("engine-addr")
                .help("Set engine addr")
                .value_name("IP:PORT")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("advertise-engine-addr")
                .long("advertise-engine-addr")
                .help("Set advertise engine addr")
                .value_name("IP:PORT")
                .required(false)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("engine-label")
                .long("engine-label")
                .help("Set engine label")
                .required(true)
                .takes_value(true),
        )
        .get_matches_from(args);

    if matches.is_present("print-sample-config") {
        let config = TiKvConfig::default();
        println!("{}", toml::to_string_pretty(&config).unwrap());
        process::exit(0);
    }

    let mut unrecognized_keys = Vec::new();
    let is_config_check = matches.is_present("config-check");

    let mut config = matches
        .value_of_os("config")
        .map_or_else(TiKvConfig::default, |path| {
            let path = Path::new(path);
            TiKvConfig::from_file(
                path,
                if is_config_check {
                    Some(&mut unrecognized_keys)
                } else {
                    None
                },
            )
            .unwrap_or_else(|e| {
                panic!(
                    "invalid auto generated configuration file {}, err {}",
                    path.display(),
                    e
                );
            })
        });

    check_engine_label(&matches);
    crate::setup::overwrite_config_with_cmd_args(&mut config, &matches);
    config.logger_compatible_adjust();

    if is_config_check {
        validate_and_persist_config(&mut config, false);
        ensure_no_unrecognized_config(&unrecognized_keys);
        println!("config check successful");
        process::exit(0)
    }

    config.raft_store.engine_store_server_helper = engine_store_server_helper as *const _ as isize;
    crate::server::run_tikv(config, engine_store_server_helper);
}

fn check_engine_label(matches: &clap::ArgMatches<'_>) {
    let engine_label = matches.value_of("engine-label").unwrap();
    let expect_engine_label = option_env!("ENGINE_LABEL_VALUE").unwrap();
    if engine_label != expect_engine_label {
        panic!(
            "`engine-label` is `{}`, expect `{}`",
            engine_label, expect_engine_label
        );
    }
}
