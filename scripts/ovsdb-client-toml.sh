#!/bin/bash

# Remove any existing client toml.
OVSDB_TOML=ovsdb_client/Cargo.toml
rm -f $OVSDB_TOML

echo "[package]
name = \"ovsdb_client\"
version = \"0.1.0\"
edition = \"2018\"
license = \"MIT\"

[dependencies]
differential_datalog = {path = \"../$1/$2_ddlog/differential_datalog\"}
libc = \"0.2.98\"
ddlog_ovsdb_adapter = {path = \"../$1/$2_ddlog/ovsdb\"}
ovs = {path = \"../ovs\"}
$2 = {path = \"../$1/$2_ddlog\", features = [\"ovsdb\"]}
memoffset = \"0.6.4\"
serde = \"1.0.126\"
serde_json = \"1.0.65\"
tokio = { version = \"1.2.0\", features = [\"full\"]}" > $OVSDB_TOML
