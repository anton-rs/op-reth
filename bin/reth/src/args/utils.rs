//! Clap parser utilities

use reth_primitives::{
    fs, AllGenesisFormats, BlockHashOrNumber, ChainSpec, B256, DEV, GOERLI, HOLESKY, MAINNET,
    SEPOLIA,
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::Duration,
};

/// Helper to parse a [Duration] from seconds
pub fn parse_duration_from_secs(arg: &str) -> eyre::Result<Duration, std::num::ParseIntError> {
    let seconds = arg.parse()?;
    Ok(Duration::from_secs(seconds))
}

/// Clap value parser for [ChainSpec]s that takes either a built-in chainspec or the path
/// to a custom one.
pub fn chain_spec_value_parser(s: &str) -> eyre::Result<Arc<ChainSpec>, eyre::Error> {
    Ok(match s {
        "mainnet" => MAINNET.clone(),
        "goerli" => GOERLI.clone(),
        "sepolia" => SEPOLIA.clone(),
        "holesky" => HOLESKY.clone(),
        "dev" => DEV.clone(),
        _ => {
            let raw = fs::read_to_string(PathBuf::from(shellexpand::full(s)?.into_owned()))?;
            serde_json::from_str(&raw)?
        }
    })
}

/// Clap value parser for [ChainSpec]s.
///
/// The value parser matches either a known chain, the path
/// to a json file, or a json formatted string in-memory. The json can be either
/// a serialized [ChainSpec] or Genesis struct.
pub fn genesis_value_parser(s: &str) -> eyre::Result<Arc<ChainSpec>, eyre::Error> {
    Ok(match s {
        "mainnet" => MAINNET.clone(),
        "goerli" => GOERLI.clone(),
        "sepolia" => SEPOLIA.clone(),
        "holesky" => HOLESKY.clone(),
        "dev" => DEV.clone(),
        _ => {
            // try to read json from path first
            let mut raw =
                match fs::read_to_string(PathBuf::from(shellexpand::full(s)?.into_owned())) {
                    Ok(raw) => raw,
                    Err(io_err) => {
                        // valid json may start with "\n", but must contain "{"
                        if s.contains('{') {
                            s.to_string()
                        } else {
                            return Err(io_err.into()) // assume invalid path
                        }
                    }
                };

            // The ethereum mainnet TTD is 58750000000000000000000, and geth serializes this
            // without quotes, because that is how golang `big.Int`s marshal in JSON. Numbers
            // are arbitrary precision in JSON, so this is valid JSON. This number is also
            // greater than a `u64`.
            //
            // Unfortunately, serde_json only supports parsing up to `u64`, resorting to `f64`
            // once `u64` overflows:
            // <https://github.com/serde-rs/json/blob/4bc1eaa03a6160593575bc9bc60c94dba4cab1e3/src/de.rs#L1411-L1415>
            // <https://github.com/serde-rs/json/blob/4bc1eaa03a6160593575bc9bc60c94dba4cab1e3/src/de.rs#L479-L484>
            // <https://github.com/serde-rs/json/blob/4bc1eaa03a6160593575bc9bc60c94dba4cab1e3/src/de.rs#L102-L108>
            //
            // serde_json does have an arbitrary precision feature, but this breaks untagged
            // enums in serde:
            // <https://github.com/serde-rs/serde/issues/2230>
            // <https://github.com/serde-rs/serde/issues/1183>
            //
            // To solve this, we surround the mainnet TTD with quotes, which our custom Visitor
            // accepts.
            if raw.contains("58750000000000000000000") &&
                !raw.contains("\"58750000000000000000000\"")
            {
                raw = raw.replacen("58750000000000000000000", "\"58750000000000000000000\"", 1);
            }

            // both serialized Genesis and ChainSpec structs supported
            let genesis: AllGenesisFormats = serde_json::from_str(&raw)?;

            Arc::new(genesis.into())
        }
    })
}

/// Parse [BlockHashOrNumber]
pub fn hash_or_num_value_parser(value: &str) -> eyre::Result<BlockHashOrNumber, eyre::Error> {
    match B256::from_str(value) {
        Ok(hash) => Ok(BlockHashOrNumber::Hash(hash)),
        Err(_) => Ok(BlockHashOrNumber::Number(value.parse()?)),
    }
}

/// Error thrown while parsing a socket address.
#[derive(thiserror::Error, Debug)]
pub enum SocketAddressParsingError {
    /// Failed to convert the string into a socket addr
    #[error("could not parse socket address: {0}")]
    Io(#[from] std::io::Error),
    /// Input must not be empty
    #[error("cannot parse socket address from empty string")]
    Empty,
    /// Failed to parse the address
    #[error("could not parse socket address from {0}")]
    Parse(String),
    /// Failed to parse port
    #[error("could not parse port: {0}")]
    Port(#[from] std::num::ParseIntError),
}

/// Parse a [SocketAddr] from a `str`.
///
/// The following formats are checked:
///
/// - If the value can be parsed as a `u16` or starts with `:` it is considered a port, and the
/// hostname is set to `localhost`.
/// - If the value contains `:` it is assumed to be the format `<host>:<port>`
/// - Otherwise it is assumed to be a hostname
///
/// An error is returned if the value is empty.
pub fn parse_socket_address(value: &str) -> eyre::Result<SocketAddr, SocketAddressParsingError> {
    if value.is_empty() {
        return Err(SocketAddressParsingError::Empty)
    }

    if let Some(port) = value.strip_prefix(':').or_else(|| value.strip_prefix("localhost:")) {
        let port: u16 = port.parse()?;
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }
    if let Ok(port) = value.parse::<u16>() {
        return Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
    }
    value
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| SocketAddressParsingError::Parse(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::Rng;
    use reth_primitives::{
        hex, Address, ChainConfig, ChainSpecBuilder, Genesis, GenesisAccount, U256,
    };
    use secp256k1::rand::thread_rng;
    use std::collections::HashMap;

    #[test]
    fn parse_known_chain_spec() {
        for chain in ["mainnet", "sepolia", "goerli", "holesky"] {
            chain_spec_value_parser(chain).unwrap();
            genesis_value_parser(chain).unwrap();
        }
    }

    #[test]
    fn parse_chain_spec_from_memory() {
        let custom_genesis_from_json = r#"
{
    "nonce": "0x0",
    "timestamp": "0x653FEE9E",
    "gasLimit": "0x1388",
    "difficulty": "0x0",
    "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "coinbase": "0x0000000000000000000000000000000000000000",
    "alloc": {
        "0x6Be02d1d3665660d22FF9624b7BE0551ee1Ac91b": {
            "balance": "0x21"
        }
    },
    "number": "0x0",
    "gasUsed": "0x0",
    "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "config": {
        "chainId": 2600,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true,
        "shanghaiTime": 0
    }
}
"#;

        let chain_from_json = genesis_value_parser(custom_genesis_from_json).unwrap();

        // using structs
        let config = ChainConfig {
            chain_id: 2600,
            homestead_block: Some(0),
            eip150_block: Some(0),
            eip155_block: Some(0),
            eip158_block: Some(0),
            byzantium_block: Some(0),
            constantinople_block: Some(0),
            petersburg_block: Some(0),
            istanbul_block: Some(0),
            berlin_block: Some(0),
            london_block: Some(0),
            shanghai_time: Some(0),
            terminal_total_difficulty: Some(U256::ZERO),
            terminal_total_difficulty_passed: true,
            ..Default::default()
        };
        let genesis = Genesis {
            config,
            nonce: 0,
            timestamp: 1698688670,
            gas_limit: 5000,
            difficulty: U256::ZERO,
            mix_hash: B256::ZERO,
            coinbase: Address::ZERO,
            ..Default::default()
        };

        // seed accounts after genesis struct created
        let address = hex!("6Be02d1d3665660d22FF9624b7BE0551ee1Ac91b").into();
        let account = GenesisAccount::default().with_balance(U256::from(33));
        let genesis = genesis.extend_accounts(HashMap::from([(address, account)]));

        let custom_genesis_from_struct = serde_json::to_string(&genesis).unwrap();
        let chain_from_struct = genesis_value_parser(&custom_genesis_from_struct).unwrap();
        assert_eq!(chain_from_json.genesis(), chain_from_struct.genesis());

        // chain spec
        let chain_spec = ChainSpecBuilder::default()
            .chain(2600.into())
            .genesis(genesis)
            .cancun_activated()
            .build();

        let chain_spec_json = serde_json::to_string(&chain_spec).unwrap();
        let custom_genesis_from_spec = genesis_value_parser(&chain_spec_json).unwrap();

        assert_eq!(custom_genesis_from_spec.chain(), chain_from_struct.chain());
    }

    #[test]
    fn parse_socket_addresses() {
        for value in ["localhost:9000", ":9000", "9000"] {
            let socket_addr = parse_socket_address(value)
                .unwrap_or_else(|_| panic!("could not parse socket address: {value}"));

            assert!(socket_addr.ip().is_loopback());
            assert_eq!(socket_addr.port(), 9000);
        }
    }

    #[test]
    fn parse_socket_address_random() {
        let port: u16 = thread_rng().gen();

        for value in [format!("localhost:{port}"), format!(":{port}"), port.to_string()] {
            let socket_addr = parse_socket_address(&value)
                .unwrap_or_else(|_| panic!("could not parse socket address: {value}"));

            assert!(socket_addr.ip().is_loopback());
            assert_eq!(socket_addr.port(), port);
        }
    }
}
