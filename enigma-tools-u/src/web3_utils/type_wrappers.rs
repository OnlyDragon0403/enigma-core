use bigint;
pub use rlp::{Encodable, RlpStream, encode};
use web3::types::{Log, TransactionReceipt, Block, H256, H64, H160, U256, U128, H2048, Bytes};
use ethabi::{Token, Bytes as AbiBytes, RawLog};
use web3::contract::tokens::Tokenizable;

pub trait IntoBigint<T> {
    fn bigint(self) -> T;
}

impl IntoBigint<bigint::H64> for H64 { fn bigint(self) -> bigint::H64 { bigint::H64(self.0) } }

impl IntoBigint<bigint::H160> for H160 { fn bigint(self) -> bigint::H160 { bigint::H160(self.0) } }

impl IntoBigint<bigint::H256> for H256 { fn bigint(self) -> bigint::H256 { bigint::H256(self.0) } }

impl IntoBigint<bigint::U256> for U256 { fn bigint(self) -> bigint::U256 { bigint::U256(self.0) } }

impl IntoBigint<bigint::U128> for U128 { fn bigint(self) -> bigint::U128 { bigint::U128(self.0) } }

impl IntoBigint<bigint::H2048> for H2048 { fn bigint(self) -> bigint::H2048 { bigint::H2048(self.0) } }

impl IntoBigint<bigint::B256> for Bytes { fn bigint(self) -> bigint::B256 { bigint::B256::new(&self.0) } }

#[derive(Debug, Clone)]
pub struct LogWrapper(pub Log);

impl Into<Vec<Token>> for LogWrapper {
    fn into(self) -> Vec<Token> {
        vec![
            Tokenizable::into_token(self.0.address),
            Tokenizable::into_token(self.0.topics),
            Token::Bytes(self.0.data.0),
        ]
    }
}

impl Encodable for LogWrapper {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(3);
        s.append(&self.0.address.bigint());
        s.append_list(&self.0.topics.iter().map(|t| t.bigint()).collect::<Vec<bigint::H256>>());
        s.append(&self.0.data.0);
    }
}


#[derive(Debug, Clone)]
pub struct BlockHeaderWrapper(pub Block<H256>);

impl Encodable for BlockHeaderWrapper {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(15);
        s.append(&self.0.parent_hash.bigint());
        s.append(&self.0.uncles_hash.bigint());
        s.append(&self.0.author.bigint());
        s.append(&self.0.state_root.bigint());
        s.append(&self.0.transactions_root.bigint());
        s.append(&self.0.receipts_root.bigint());
        s.append(&self.0.logs_bloom.bigint());
        s.append(&self.0.difficulty.bigint());
        s.append(&self.0.number.unwrap().bigint());
        s.append(&self.0.gas_limit.bigint());
        s.append(&self.0.gas_used.bigint());
        s.append(&self.0.timestamp.bigint());
        s.append(&self.0.extra_data.clone().bigint());
        s.append(&H256::from(0).bigint()); // TODO: missing from web3
        s.append(&H256::from(0).bigint()); // TODO: missing from web3
    }
}

#[derive(Debug, Clone)]
pub struct BlockHeaders(pub Vec<BlockHeaderWrapper>);
impl Encodable for BlockHeaders {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1);
        s.append_list(&self.0);
    }
}

#[derive(Debug, Clone)]
pub struct ReceiptWrapper {
    pub receipt: TransactionReceipt,
    pub block: Block<H256>,
}

impl Encodable for ReceiptWrapper {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1);
        s.append(&self.block.state_root.bigint());
        s.append(&self.receipt.cumulative_gas_used.bigint());
        s.append(&self.block.logs_bloom.bigint());
        s.append_list(&self.receipt.logs.iter().map(|l| LogWrapper(l.clone())).collect::<Vec<LogWrapper>>());
    }
}

#[derive(Debug, Clone)]
pub struct ReceiptHashesWrapper(pub Vec<H256>);

impl Encodable for ReceiptHashesWrapper {
    fn rlp_append(&self, s: &mut RlpStream) {
        s.begin_list(1);
        s.append_list(&self.0.iter().map(|h| h.bigint()).collect::<Vec<bigint::H256>>());
    }
}
