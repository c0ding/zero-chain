#[macro_use]
extern crate log;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate serde_derive;
#[cfg(test)]
#[macro_use]
extern crate matches;

use std::fs::File;
use std::path::{Path, PathBuf};
use std::io::{BufWriter, Write, BufReader, Read};
use clap::{Arg, App, SubCommand, AppSettings, ArgMatches};
use rand::{OsRng, Rng};
use proofs::{
    EncryptionKey, ProofGenerationKey, SpendingKey,
    elgamal,
    Transaction,
    setup,
    PARAMS,
    };
use primitives::{hexdisplay::{HexDisplay, AsBytesRef}, crypto::Ss58Codec};
use pairing::{bls12_381::Bls12, Field, PrimeField, PrimeFieldRepr};
use scrypto::jubjub::fs;

use bellman::groth16::{Parameters, PreparedVerifyingKey};
use polkadot_rs::{Api, Url, hexstr_to_u64};
use zprimitives::PARAMS as ZPARAMS;
use bip39::{Mnemonic, Language, MnemonicType};

mod utils;
mod config;
mod wallet;
mod transaction;
pub mod derive;
pub mod term;
pub mod ss58;
use self::ss58::EncryptionKeyBytes;
use utils::*;
use config::*;
use wallet::commands::*;
use transaction::commands::*;

//
// Global constants
//

const VERIFICATION_KEY_PATH: &str = "zeroc/verification.params";
const PROVING_KEY_PATH: &str = "zeroc/proving.params";
const DEFAULT_AMOUNT: &str = "10";
const DEFAULT_BALANCE: &str = "100";
const ALICESEED: &str = "416c696365202020202020202020202020202020202020202020202020202020";
const BOBSEED: &str = "426f622020202020202020202020202020202020202020202020202020202020";
// const BOBACCOUNTID: &str = "45e66da531088b55dcb3b273ca825454d79d2d1d5c4fa2ba4a12c1fa1ccd6389";
const ALICEDECRYPTIONKEY: &str = "b0451b0bfab2830a75216779e010e0bfd2e6d0b4e4b1270dfcdfd0d538509e02";
const DEFAULT_ENCRYPTED_BALANCE: &str = "6f4962da776a391c3b03f3e14e8156d2545f39a3ebbed675ea28859252cb006fac776c796563fcd44cc49cfaea8bb796952c266e47779d94574c10ad01754b11";

fn main() {
    let default_root_dir = get_default_root_dir();

    let matches = App::new("zeroc")
        .setting(AppSettings::SubcommandRequiredElseHelp)
        .version(crate_version!())
        .author(crate_authors!())
        .about("zeroc: Zerochain Command Line Interface")
        .arg(global_verbose_definition())
        .arg(global_quiet_difinition())
        .arg(global_color_definition())
        .arg(global_rootdir_definition(&default_root_dir))
        .subcommand(snark_commands_definition())
        .subcommand(wallet_commands_definition())
        .subcommand(tx_commands_definition())
        .subcommand(debug_commands_definition())
        .get_matches();

    let mut term = term::Term::new(config_terminal(&matches));

    let root_dir = global_rootdir_match(&default_root_dir, &matches);

    let rng = &mut OsRng::new().expect("should be able to construct RNG");

    match matches.subcommand() {
        (SNARK_COMMAND, Some(matches)) => subcommand_snark(term, root_dir, matches),
        (WALLET_COMMAND, Some(matches)) => subcommand_wallet(term, root_dir, matches, rng),
        (TX_COMMAND, Some(matches)) => subcommand_tx(term, root_dir, matches, rng),
        (DEBUG_COMMAND, Some(matches)) => subcommand_debug(term, root_dir, matches),
        _ => {
            term.error(matches.usage()).unwrap();
            ::std::process::exit(1);
        }
    }
}

//
//  Snark Sub Commands
//

const SNARK_COMMAND: &'static str = "snark";

fn subcommand_snark(mut term: term::Term, root_dir: PathBuf, matches: &ArgMatches) {
    let res = match matches.subcommand() {
        ("setup", Some(matches)) => {
            println!("Performing setup...");

            let pk_path = Path::new(matches.value_of("proving-key-path").unwrap());
            let vk_path = Path::new(matches.value_of("verification-key-path").unwrap());

            let pk_file = File::create(&pk_path)
                .map_err(|why| format!("couldn't create {}: {}", pk_path.display(), why)).unwrap();
            let vk_file = File::create(&vk_path)
                .map_err(|why| format!("couldn't create {}: {}", vk_path.display(), why)).unwrap();

            let mut bw_pk = BufWriter::new(pk_file);
            let mut bw_vk = BufWriter::new(vk_file);

            let (proving_key, prepared_vk) = setup();
            let mut v_pk = vec![];
            let mut v_vk = vec![];

            proving_key.write(&mut &mut v_pk).unwrap();
            prepared_vk.write(&mut &mut v_vk).unwrap();

            bw_pk.write(&v_pk[..])
                .map_err(|_| "Unable to write proving key data to file.".to_string()).unwrap();

            bw_vk.write(&v_vk[..])
                .map_err(|_| "Unable to write verification key data to file.".to_string()).unwrap();

            bw_pk.flush()
                .map_err(|_| "Unable to flush proving key buffer.".to_string()).unwrap();

            bw_vk.flush()
                .map_err(|_| "Unable to flush verification key buffer.".to_string()).unwrap();

            println!("Success! Output >> 'proving.params' and 'verification.params'");
        },
        _ => {
            term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    };

    // res.unwrap_or_else(|e| term.fail_with(e))
}

fn snark_commands_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(SNARK_COMMAND)
        .about("zk-snarks operations")
        .subcommand(SubCommand::with_name("setup")
            .about("Performs a trusted setup for a given constraint system")
            .arg(Arg::with_name("proving-key-path")
                .short("p")
                .long("proving-key-path")
                .help("Path of the generated proving key file")
                .value_name("FILE")
                .takes_value(true)
                .required(false)
                .default_value(PROVING_KEY_PATH)
            )
            .arg(Arg::with_name("verification-key-path")
                .short("v")
                .long("verification-key-path")
                .help("Path of the generated verification key file")
                .value_name("FILE")
                .takes_value(true)
                .required(false)
                .default_value(VERIFICATION_KEY_PATH)
            )
        )
}

//
// Wallet Sub Commands
//

const WALLET_COMMAND: &'static str = "wallet";

fn subcommand_wallet<R: Rng>(mut term: term::Term, root_dir: PathBuf, matches: &ArgMatches, rng: &mut R) {
    let res = match matches.subcommand() {
        ("init", Some(_)) => {
            // Create new wallet
            new_wallet(&mut term, root_dir, rng)
                .expect("Invalid operations of creating new wallet.");
        },
        ("list", Some(_)) => {
            // show accounts list
            show_list(&mut term, root_dir)
                .expect("Invalid operations of listing accounts.");
        },
        ("add-account", Some(_)) => {
            new_keyfile(&mut term, root_dir, rng)
                .expect("Invalid operations of creating new account.");
        },
        ("change-account", Some(_)) => {

        },
        ("recovery", Some(_)) => {
            recover(&mut term, root_dir)
                .expect("Invalid mnemonic to recover keystore.")
        },
        ("wallet-test", Some(_)) => {
            println!("Initialize key components...");
            println!("Accounts of alice and bob are fixed");

            let alice_seed = seed_to_array(ALICESEED);
            let bob_seed = seed_to_array(BOBSEED);

            let print_keys_alice = PrintKeys::generate_from_seed(alice_seed);
            let print_keys_bob = PrintKeys::generate_from_seed(bob_seed);
            let print_keys_charlie = PrintKeys::generate();

            println!(
                "
                \nSeed
                Alice: 0x{}
                Bob: 0x{}
                Charlie: 0x{}
                \nDecryption Key
                Alice: 0x{}
                Bob: 0x{}
                Charlie: 0x{}
                \nEncryption Key
                Alice: 0x{}
                Bob: 0x{}
                Charlie: 0x{}
                ",
                hex::encode(&alice_seed[..]),
                hex::encode(&print_keys_bob.seed[..]),
                hex::encode(&print_keys_charlie.seed[..]),
                hex::encode(&print_keys_alice.decryption_key[..]),
                hex::encode(&print_keys_bob.decryption_key[..]),
                hex::encode(&print_keys_charlie.decryption_key[..]),
                hex::encode(&print_keys_alice.encryption_key[..]),
                hex::encode(&print_keys_bob.encryption_key[..]),
                hex::encode(&print_keys_charlie.encryption_key[..]),
            );
        },
        ("inspect", Some(sub_matches)) => {
            let uri = sub_matches.value_of("uri")
                .expect("URI parameter is required; qed");
        },
        _ => {
            term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    };

    // res.unwrap_or_else(|e| term.fail_with(e))
}

fn wallet_commands_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(WALLET_COMMAND)
        .about("wallet operations")
        .subcommand(SubCommand::with_name("wallet-test")
            .about("Initialize key components")
        )
        .subcommand(SubCommand::with_name("init")
            .about("Initialize your wallet")
        )
        .subcommand(SubCommand::with_name("list")
            .about("Show accounts list.")
        )
        .subcommand(SubCommand::with_name("add-account")
            .about("Add a new account")
        )
        .subcommand(SubCommand::with_name("change-account")
            .about("Change default account")
            .arg(Arg::with_name("account_name")
                .short("n")
                .long("name")
                .help("A new account name that you have in your keystore.")
                .takes_value(true)
                .required(true)
            )
        )
        .subcommand(SubCommand::with_name("recovery")
            .about("Recover keystore from mnemonic.")
        )
        .subcommand(SubCommand::with_name("inspect")
            .about("Gets a encryption key and a SS58 address from the provided Secret URI")
            .arg(Arg::with_name("uri")
                .short("u")
                .long("uri")
                .help("A Key URI to be inspected like a secret seed, SS58 or public URI.")
                .required(true)
            )
        )
}

//
// Transaction Sub Commands
//

const TX_COMMAND: &'static str = "tx";

fn tx_arg_seed_match<'a>(matches: &ArgMatches<'a>) -> Vec<u8> {
    hex::decode(matches.value_of("sender-seed")
        .expect("Seed parameter is required; qed"))
        .expect("should be decoded to hex.")
}

fn tx_arg_recipient_address_match<'a>(matches: &ArgMatches<'a>) -> [u8; 32] {
    let recipient_address = matches.value_of("recipient-address")
        .expect("Recipient's address is required; qed");

    let recipient_enc_key = EncryptionKeyBytes::from_ss58check(recipient_address)
        .expect("The string should be a properly encoded SS58Check address.");

    recipient_enc_key.0
}

fn tx_arg_amount_match<'a>(matches: &ArgMatches<'a>) -> u32 {
    let amount_str = matches.value_of("amount")
        .expect("Amount parameter is required; qed");

    let amount: u32 = amount_str.parse()
        .expect("should be parsed to u32 number; qed");

    amount
}

fn tx_arg_url_match<'a>(matches: &ArgMatches<'a>) -> Url {
    match matches.value_of("url") {
        Some(u) => Url::Custom(u.to_string()),
        None => Url::Local,
    }
}

fn subcommand_tx<R: Rng>(mut term: term::Term, root_dir: PathBuf, matches: &ArgMatches, rng: &mut R) {
    let res = match matches.subcommand() {
        ("send", Some(sub_matches)) => {
            // let seed = tx_arg_seed_match(&sub_matches);
            let recipient_enc_key = tx_arg_recipient_address_match(&sub_matches);
            let amount = tx_arg_amount_match(&sub_matches);
            let url = tx_arg_url_match(&sub_matches);

            println!("Preparing paramters...");

            let api = Api::init(url);
            // let alpha = fs::Fs::rand(rng);
            let alpha = fs::Fs::zero(); // TODO

            let buf_pk = read_zk_params_with_path(PROVING_KEY_PATH);
            let buf_vk = read_zk_params_with_path(VERIFICATION_KEY_PATH);

            let proving_key = Parameters::<Bls12>::read(&mut &buf_pk[..], true)
                .expect("should be casted to Parameters<Bls12> type.");
            let prepared_vk = PreparedVerifyingKey::<Bls12>::read(&mut &buf_vk[..])
                .expect("should ne casted to PreparedVerifyingKey<Bls12> type");

            let fee_str = api.get_storage("ConfTransfer", "TransactionBaseFee", None)
                .expect("should be fetched TransactionBaseFee from ConfTransfer module of Zerochain.");
            let fee = hexstr_to_u64(fee_str) as u32;

            let spending_key = spending_key_from_keystore(&mut term, root_dir)
                .expect("should load from keystore.");

            let decryption_key = ProofGenerationKey::<Bls12>::from_spending_key(&spending_key, &PARAMS)
                .into_decryption_key()
                .expect("should be generated decryption key from seed.");

            let mut decrypted_key = [0u8; 32];
            decryption_key.0.into_repr().write_le(&mut &mut decrypted_key[..])
                .expect("should be casted as bytes-array.");

            let balance_query = BalanceQuery::get_balance_from_decryption_key(&decrypted_key[..], api.clone());
            let remaining_balance = balance_query.decrypted_balance - amount - fee;
            assert!(balance_query.decrypted_balance >= amount + fee, "Not enough balance you have");

            let recipient_account_id = EncryptionKey::<Bls12>::read(&mut &recipient_enc_key[..], &PARAMS)
                .expect("should be casted to EncryptionKey<Bls12> type.");
            let encrypted_balance = elgamal::Ciphertext::read(&mut &balance_query.encrypted_balance[..], &*PARAMS)
                .expect("should be casted to Ciphertext type.");

            println!("Computing zk proof...");
            let tx = Transaction::gen_tx(
                amount,
                remaining_balance,
                alpha,
                &proving_key,
                &prepared_vk,
                &recipient_account_id,
                &spending_key,
                encrypted_balance,
                rng,
                fee
                )
            .expect("fails to generate the tx");

            println!("Start submitting a transaction to Zerochain...");

            submit_tx(&tx, &api, rng);

            println!("Remaining balance is {}", remaining_balance);

        },
        ("balance", Some(sub_matches)) => {
            println!("Getting encrypted balance from zerochain");
            // let url = Url::Custom("ws://0.0.0.0:9944".to_string());
            // let api = Api::init(url);
            let api = Api::init(Url::Local);
            let decryption_key_vec = hex::decode(sub_matches.value_of("decryption-key")
                .expect("Decryption key parameter is required; qed"))
                .expect("should be decoded to hex.");

            let balance_query = BalanceQuery::get_balance_from_decryption_key(&decryption_key_vec[..], api);

            println!("Decrypted balance: {}", balance_query.decrypted_balance);
            println!("Encrypted balance: {}", balance_query.encrypted_balance_str);
            println!("Encrypted pending transfer: {}", balance_query.pending_transfer_str);
        },
        _ => {
            term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    };

    // res.unwrap_or_else(|e| term.fail_with(e))

}

fn tx_commands_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(TX_COMMAND)
        .about("transaction operations")
        .subcommand(SubCommand::with_name("send")
            .about("Submit extrinsic to the substrate nodes")
            .arg(Arg::with_name("amount")
                .short("a")
                .long("amount")
                .help("The coin amount for the confidential transfer. (default: 10)")
                .takes_value(true)
                .required(false)
            )
            .arg(Arg::with_name("recipient-address")
                .short("to")
                .long("recipient-address")
                .help("Recipient's SS58-encoded address")
                .takes_value(true)
                .required(false)
            )
            .arg(Arg::with_name("url")
                .short("u")
                .long("url")
                .help("Endpoint to connect zerochain nodes")
                .takes_value(true)
                .required(false)
            )
        )
        .subcommand(SubCommand::with_name("balance")
            .about("Get current balance stored in ConfTransfer module")
            .arg(Arg::with_name("decryption-key")
                .short("d")
                .long("decryption-key")
                .help("Your decription key")
                .takes_value(true)
                .required(true)
                .default_value(ALICEDECRYPTIONKEY)
            )
        )
}

//
// Debug Sub Commands
//

const DEBUG_COMMAND: &'static str = "debug";

fn subcommand_debug(mut term: term::Term, root_dir: PathBuf, matches: &ArgMatches) {
    let res = match matches.subcommand() {
        ("key-init", Some(sub_matches)) => {
            let lang = Language::English;
            // create a new randomly generated mnemonic phrase
            let mnemonic = Mnemonic::new(MnemonicType::Words12, lang);
            PrintKeys::print_from_phrase(mnemonic.phrase(), None, lang);
        },
        ("print-tx", Some(sub_matches)) => {
            println!("Generate transaction...");

            let sender_seed = hex::decode(sub_matches.value_of("sender-privatekey").unwrap()).unwrap();
            let recipient_seed  = hex::decode(sub_matches.value_of("recipient-privatekey").unwrap()).unwrap();

            let sender_address = get_address(&sender_seed[..]).unwrap();
            let recipient_address = get_address(&recipient_seed[..]).unwrap();

            println!("Private Key(Sender): 0x{}\nAddress(Sender): 0x{}\n",
                HexDisplay::from(&sender_seed),
                HexDisplay::from(&sender_address),
            );

            println!("Private Key(Recipient): 0x{}\nAddress(Recipient): 0x{}\n",
                HexDisplay::from(&recipient_seed),
                HexDisplay::from(&recipient_address),
            );

            let pk_path = Path::new(sub_matches.value_of("proving-key-path").unwrap());
            let vk_path = Path::new(sub_matches.value_of("verification-key-path").unwrap());

            let pk_file = File::open(&pk_path)
                .map_err(|why| format!("couldn't open {}: {}", pk_path.display(), why)).unwrap();
            let vk_file = File::open(&vk_path)
                .map_err(|why| format!("couldn't open {}: {}", vk_path.display(), why)).unwrap();

            let mut reader_pk = BufReader::new(pk_file);
            let mut reader_vk = BufReader::new(vk_file);

            let mut buf_pk = vec![];
            reader_pk.read_to_end(&mut buf_pk)
                .map_err(|why| format!("couldn't read {}: {}", pk_path.display(), why)).unwrap();

            let mut buf_vk = vec![];
            reader_vk.read_to_end(&mut buf_vk)
                .map_err(|why| format!("couldn't read {}: {}", vk_path.display(), why)).unwrap();


            let amount_str = sub_matches.value_of("amount").unwrap();
            let amount: u32 = amount_str.parse().unwrap();
            // let fee_str = sub_matches.value_of("fee").unwrap();
            // let fee: u32 = fee_str.parse().unwrap();
            let fee = 1 as u32;

            let balance_str = sub_matches.value_of("balance").unwrap();
            let balance: u32 = balance_str.parse().unwrap();

            println!("Transaction >>");

            let rng = &mut OsRng::new().expect("should be able to construct RNG");

            // let alpha = fs::Fs::rand(rng);
            let alpha = fs::Fs::zero(); // TODO

            let proving_key = Parameters::<Bls12>::read(&mut &buf_pk[..], true).unwrap();
            let prepared_vk = PreparedVerifyingKey::<Bls12>::read(&mut &buf_vk[..]).unwrap();

            let address_recipient = EncryptionKey::<Bls12>::from_seed(&recipient_seed[..], &PARAMS).unwrap();

            let ciphertext_balance_a = sub_matches.value_of("encrypted-balance").unwrap();
            let ciphertext_balance_v = hex::decode(ciphertext_balance_a).unwrap();
            let ciphertext_balance = elgamal::Ciphertext::read(&mut &ciphertext_balance_v[..], &*PARAMS).unwrap();

            let remaining_balance = balance - amount - fee;

            let tx = Transaction::gen_tx(
                            amount,
                            remaining_balance,
                            alpha,
                            &proving_key,
                            &prepared_vk,
                            &address_recipient,
                            &SpendingKey::<Bls12>::from_seed(&sender_seed[..]),
                            ciphertext_balance,
                            rng,
                            fee
                    ).expect("fails to generate the tx");

            // println!(
            //     "
            //     \nEncrypted fee by sender: 0x{}
            //     \nzkProof: 0x{}
            //     \nEncrypted amount by sender: 0x{}
            //     \nEncrypted amount by recipient: 0x{}
            //     ",
            //     HexDisplay::from(&tx.enc_fee as &AsBytesRef),
            //     HexDisplay::from(&&tx.proof[..] as &AsBytesRef),
            //     HexDisplay::from(&tx.enc_val_sender as &AsBytesRef),
            //     HexDisplay::from(&tx.enc_val_recipient as &AsBytesRef),
            // );
            println!(
                "
                \nzkProof(Alice): 0x{}
                \naddress_sender(Alice): 0x{}
                \naddress_recipient(Alice): 0x{}
                \nvalue_sender(Alice): 0x{}
                \nvalue_recipient(Alice): 0x{}
                \nrvk(Alice): 0x{}
                \nrsk(Alice): 0x{}
                \nEncrypted fee by sender: 0x{}
                ",
                HexDisplay::from(&&tx.proof[..] as &dyn AsBytesRef),
                HexDisplay::from(&tx.address_sender as &dyn AsBytesRef),
                HexDisplay::from(&tx.address_recipient as &dyn AsBytesRef),
                HexDisplay::from(&tx.enc_amount_sender as &dyn AsBytesRef),
                HexDisplay::from(&tx.enc_amount_recipient as &dyn AsBytesRef),
                HexDisplay::from(&tx.rvk as &dyn AsBytesRef),
                HexDisplay::from(&tx.rsk as &dyn AsBytesRef),
                HexDisplay::from(&tx.enc_fee as &dyn AsBytesRef),
            );
        },
        _ => {
            term.error(matches.usage()).unwrap();
            ::std::process::exit(1)
        }
    };
}

fn debug_commands_definition<'a, 'b>() -> App<'a, 'b> {
    SubCommand::with_name(DEBUG_COMMAND)
        .about("debug operations")
        .subcommand(SubCommand::with_name("key-init")
            .about("Print a keypair")
        )
        .subcommand(SubCommand::with_name("print-tx")
            .about("Show transaction components for sending it from a browser")
            .arg(Arg::with_name("proving-key-path")
                .short("p")
                .long("proving-key-path")
                .help("Path of the proving key file")
                .value_name("FILE")
                .takes_value(true)
                .required(false)
                .default_value(PROVING_KEY_PATH)
            )
            .arg(Arg::with_name("verification-key-path")
                .short("v")
                .long("verification-key-path")
                .help("Path of the generated verification key file")
                .value_name("FILE")
                .takes_value(true)
                .required(false)
                .default_value(VERIFICATION_KEY_PATH)
            )
            .arg(Arg::with_name("amount")
                .short("a")
                .long("amount")
                .help("The coin amount for the confidential transfer. (default: 10)")
                .takes_value(true)
                .required(false)
                .default_value(DEFAULT_AMOUNT)
            )
            .arg(Arg::with_name("balance")
                .short("b")
                .long("balance")
                .help("The coin balance for the confidential transfer.")
                .takes_value(true)
                .required(false)
                .default_value(DEFAULT_BALANCE)
            )
            .arg(Arg::with_name("sender-privatekey")
                .short("s")
                .long("sender-privatekey")
                .help("Sender's private key. (default: Alice)")
                .takes_value(true)
                .required(false)
                .default_value(ALICESEED)
            )
            .arg(Arg::with_name("recipient-privatekey")
                .short("r")
                .long("recipient-privatekey")
                .help("Recipient's private key. (default: Bob)")
                .takes_value(true)
                .required(false)
                .default_value(BOBSEED)
            )
            .arg(Arg::with_name("encrypted-balance")
                .short("e")
                .long("encrypted-balance")
                .help("Encrypted balance by sender stored in on-chain")
                .takes_value(true)
                .required(false)
                .default_value(DEFAULT_ENCRYPTED_BALANCE)
            )
        )
}
