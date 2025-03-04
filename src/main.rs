use std::{
    collections::BTreeSet,
    io::{Cursor, Write},
    str::FromStr,
};

use bdk_esplora::{EsploraExt, esplora_client};
use bdk_wallet::{
    KeychainKind, SignOptions, Wallet,
    bitcoin::{
        Address, Amount, FeeRate, Network, PrivateKey, PublicKey, consensus::Encodable,
        key::Secp256k1,
    },
    keys::DescriptorPublicKey,
    miniscript::{Descriptor, Miniscript, Segwitv0},
};

const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const NETWORK: Network = Network::Regtest;
const ESPLORA_URL: &str = "http://127.0.0.1:3002";

fn main() -> Result<(), anyhow::Error> {
    let secp = Secp256k1::new();

    // Генерируем приватный ключ
    let private_key = PrivateKey::generate(NETWORK);
    let public_key = PublicKey::from_private_key(&secp, &private_key);

    // Создаем минискрипт and_v(v:pk(P),older(1))
    let miniscript_str = format!("and_v(v:pk({}),older(1))", public_key.to_string());
    let miniscript = Miniscript::<DescriptorPublicKey, Segwitv0>::from_str(&miniscript_str)?;

    // Создаем дескриптор из минискрипта
    let wsh_descriptor = format!("wsh({})", miniscript.to_string());
    let descriptor = Descriptor::from_str(&wsh_descriptor)?;

    // Создаём кошелёк
    let mut wallet = Wallet::create_single(descriptor)
        .network(NETWORK)
        .create_wallet_no_persist()?;

    // Задаём хелпер для синка и синхронизируем изначально
    print!("Syncing...");
    let client = esplora_client::Builder::new(ESPLORA_URL).build_blocking();

    let sync = |wallet: &mut Wallet| {
        let request = wallet.start_full_scan().inspect({
            let mut stdout = std::io::stdout();
            let mut once = BTreeSet::<KeychainKind>::new();
            move |keychain, spk_i, _| {
                if once.insert(keychain) {
                    print!("\nScanning keychain [{:?}] ", keychain);
                }
                print!(" {:<3}", spk_i);
                stdout.flush().expect("must flush")
            }
        });

        let update = client
            .full_scan(request, STOP_GAP, PARALLEL_REQUESTS)
            .expect("full scan");

        wallet.apply_update(update).expect("update");
    };
    sync(&mut wallet);

    // Получаем адрес для депозита
    let address_info = wallet.next_unused_address(KeychainKind::External);
    let deposit_address = address_info.address;

    println!("Deposit address: {}", deposit_address);

    // Ожидаем, что пользователь отправит монеты на этот адрес через Bitcoin Core CLI:
    // bitcoin-cli -regtest sendtoaddress <deposit_address> 0.01
    println!("Please send some coins to the deposit address and press Enter to continue...");
    let mut buffer = String::new();
    std::io::stdin().read_line(&mut buffer)?;

    // Синхронизируем кошелек, чтобы найти депозит
    sync(&mut wallet);

    // Проверяем, что у нас есть средства
    let balance = wallet.balance();
    println!("Wallet balance: {} sats", balance.confirmed);

    if balance.confirmed == Amount::from_sat(0) {
        anyhow::bail!("No funds in wallet. Please send coins to the deposit address.");
    }

    // Создаем транзакцию, отправляющую все монеты на другой адрес
    // Создаем тестовый адрес получателя (обычно это будет другой адрес)
    let recipient = Address::from_str("bcrt1q6rz28mcfaxtmd6v789l9rrlrusdprr9pz3cppk")?;

    // Сначала создаем транзакцию без подписи
    let mut tx_builder = wallet.build_tx();
    tx_builder
        .add_recipient(
            recipient.assume_checked().script_pubkey(),
            Amount::from_sat((balance.confirmed.to_sat() / 2) as u64),
        )
        .fee_rate(FeeRate::from_sat_per_vb(2).unwrap());

    let mut psbt = tx_builder.finish()?;

    // Подписываем транзакцию
    wallet.sign(&mut psbt, SignOptions::default())?;

    // Извлекаем сырую транзакцию
    let tx = psbt.extract_tx()?;
    let mut tx_bytes = vec![];
    tx.consensus_encode(&mut Cursor::new(&mut tx_bytes))?;
    let tx_hex = tx_bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    println!("Signed transaction (hex): {}", tx_hex);
    println!("Transaction ID: {}", tx.compute_txid());

    // Попытка отправить транзакцию - она должна быть отклонена из-за timelock (older(1))
    match client.broadcast(&tx) {
        Ok(_) => println!("Transaction broadcasted successfully (unexpected!)"),
        Err(e) => println!(
            "Transaction broadcast failed as expected due to timelock: {}",
            e
        ),
    }

    // Майним блок и пробуем еще раз
    println!("\nMining a block to satisfy the timelock...");
    println!(
        "Please mine a block using 'bitcoin-cli -regtest generatetoaddress 1 <address>' and press Enter to continue..."
    );
    buffer.clear();
    std::io::stdin().read_line(&mut buffer)?;

    // Синхронизируем кошелек после майнинга блока
    sync(&mut wallet);

    // Пробуем снова транслировать ту же транзакцию
    match client.broadcast(&tx) {
        Ok(_) => println!("Transaction broadcasted successfully after mining a block!"),
        Err(e) => println!("Transaction broadcast still failed: {}", e),
    }

    println!("\nTest complete! The transaction with and_v(v:pk(P),older(1)) script requires:");
    println!("1. A valid signature from the private key");
    println!("2. At least 1 block to be mined after the UTXO was created (older(1))");

    Ok(())
}
