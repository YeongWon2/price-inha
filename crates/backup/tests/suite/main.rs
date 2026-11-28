//! pi-backup 시험 묶음 (T19). GitHub는 로컬 가짜 서버, age 키는 시험 안에서 만든다.
//! (5차) 보관 파일 담기 · 되살리기는 `archive.rs`(결정 31).
//! (6차) 설정 파일 담기는 `config_files.rs`(gzip은 결정 33을 미뤄 빠짐).
//! (6차) 2026-11-27 백로그 작업을 합쳐 잠금.

mod archive;
mod common;
mod config_files;
mod failure;
mod late_retention;
mod partial_retention;
mod refuse;
mod restore;
mod restore_temp_root;
mod retention;
mod temp_root;
mod token_verify;
mod upload;
mod verify;
