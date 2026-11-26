//! pi-backup 시험 묶음 (T19). GitHub는 로컬 가짜 서버, age 키는 시험 안에서 만든다.
//! (5차) 보관 파일 담기 · 되살리기는 `archive.rs`(결정 31).

mod archive;
mod common;
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
