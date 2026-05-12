//! Claude-specific token account help text. The shared
//! `TokenAccountStore` provides the storage; this module supplies the
//! display strings the React settings pane surfaces under the token
//! accounts row.

pub const TOKEN_ACCOUNT_HELP: &str = "\
Paste an OAuth bearer (starts with `sk-ant-oat`) or a Web `sessionKey=...` \
cookie value. Lines starting with `Cookie:` or `Bearer` are accepted; the \
prefix is stripped.";

pub const TOKEN_ACCOUNT_TITLE: &str = "Claude accounts";
