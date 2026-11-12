//! Offstage の取り込み。画面と通信に依存しない。
//!
//! 構想は docs/offstage/concept.md。分類 (classify)・洗い出し (scan)・記録と前回との比較 (store)・
//! 会話との結び付け (link)・検索 (query)・リポジトリ探し (repos)・取り込みの流れ (pipeline)。

pub mod classify;
pub mod scan;
pub mod link;
pub mod pipeline;
pub mod query;
pub mod repos;
pub mod store;
