use super::*;
use crate::output::{decode_utf16le_creator, json_escape};
use clap::Parser;

#[test]
fn decode_creator_ascii() {
    let mut buf = vec![0u8; 512];
    let text = b"hello";
    for (i, &b) in text.iter().enumerate() {
        buf[i * 2] = b;
        buf[i * 2 + 1] = 0;
    }
    let decoded = decode_utf16le_creator(&buf);
    assert_eq!(decoded, "hello");
}

#[test]
fn decode_creator_no_terminator() {
    let buf = vec![0u8; 512];
    let decoded = decode_utf16le_creator(&buf);
    assert_eq!(decoded, "");
}

#[test]
fn decode_creator_partial() {
    let mut buf = vec![0u8; 512];
    buf[0] = b'a';
    buf[1] = 0;
    buf[2] = b'b';
    buf[3] = 0;
    let decoded = decode_utf16le_creator(&buf);
    assert_eq!(decoded, "ab");
}

#[test]
fn json_escape_basic() {
    assert_eq!(json_escape("hello"), r#""hello""#);
    assert_eq!(json_escape("he\"llo"), r#""he\"llo""#);
    assert_eq!(json_escape("a\nb"), r#""a\nb""#);
}

#[test]
fn json_escape_control() {
    let s = json_escape("\x01");
    assert_eq!(s, r#""\u0001""#);
}

#[test]
fn clap_info_help_does_not_panic() {
    let result = Cli::try_parse_from(["vhdx-tool", "info", "--help"]);
    assert!(result.is_err());
}

#[test]
fn clap_info_no_file_ok() {
    // file is now optional, so parsing without it should succeed
    let result = Cli::try_parse_from(["vhdx-tool", "info"]).unwrap();
    match result.command {
        Commands::Info(args) => {
            assert!(args.file.is_none());
        }
        _ => panic!("expected Info command"),
    }
}

#[test]
fn clap_info_with_file() {
    let result = Cli::try_parse_from(["vhdx-tool", "info", "test.vhdx"]).unwrap();
    match result.command {
        Commands::Info(args) => {
            assert_eq!(args.file.unwrap().to_str().unwrap(), "test.vhdx");
            assert_eq!(args.format, "text");
        }
        _ => panic!("expected Info command"),
    }
}

#[test]
fn clap_info_json_format() {
    let result =
        Cli::try_parse_from(["vhdx-tool", "info", "test.vhdx", "--format", "json"]).unwrap();
    match result.command {
        Commands::Info(args) => {
            assert_eq!(args.format, "json");
        }
        _ => panic!("expected Info command"),
    }
}
