#[allow(dead_code)]
mod helpers;

use {
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

use helpers::{
    setup, setup_mint_and_extra_metas, initialize_rate_limit,
    create_ata, mint_tokens, build_transfer_with_hook_ix,
};

#[test]
fn test_transfer_hook() {
    let (mut svm, payer, program_id) = setup();
    let mint = Keypair::new();

    setup_mint_and_extra_metas(&mut svm, &payer, &mint, &program_id);

    let recipient = Keypair::new();
    svm.airdrop(&recipient.pubkey(), 1_000_000_000).unwrap();

    let source_ata = create_ata(&mut svm, &payer, &payer.pubkey(), &mint.pubkey());
    let dest_ata = create_ata(&mut svm, &payer, &recipient.pubkey(), &mint.pubkey());

    let mint_amount = 1_000_000u64;
    mint_tokens(&mut svm, &payer, &mint.pubkey(), &source_ata, mint_amount);

    let transfer_ix = build_transfer_with_hook_ix(
        &source_ata, &dest_ata, &mint.pubkey(), &payer.pubkey(), &program_id, 100, 9,
    );

    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[transfer_ix], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();

    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "Transfer with hook failed: {:?}", res.err());
}

#[test]
fn test_transfer_hook_rate_limit_exceeded() {
    let (mut svm, payer, program_id) = setup();
    let mint = Keypair::new();

    setup_mint_and_extra_metas(&mut svm, &payer, &mint, &program_id);

    let recipient = Keypair::new();
    svm.airdrop(&recipient.pubkey(), 1_000_000_000).unwrap();

    let source_ata = create_ata(&mut svm, &payer, &payer.pubkey(), &mint.pubkey());
    let dest_ata = create_ata(&mut svm, &payer, &recipient.pubkey(), &mint.pubkey());

    // Mint more than the rate limit so we have enough tokens
    mint_tokens(&mut svm, &payer, &mint.pubkey(), &source_ata, 2_000_000);

    // First transfer: exactly at the limit - should succeed
    let ix1 = build_transfer_with_hook_ix(
        &source_ata, &dest_ata, &mint.pubkey(), &payer.pubkey(), &program_id, 1_000_000, 9,
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix1], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_ok(), "Transfer at limit should succeed: {:?}", res.err());

    // Second transfer: 1 token more - should fail with RateLimitExceeded
    let ix2 = build_transfer_with_hook_ix(
        &source_ata, &dest_ata, &mint.pubkey(), &payer.pubkey(), &program_id, 1, 9,
    );
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix2], Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&payer]).unwrap();
    let res = svm.send_transaction(tx);
    assert!(res.is_err(), "Transfer exceeding rate limit should fail");
}

#[test]
fn test_rate_limit_is_per_owner() {
    let (mut svm, payer, program_id) = setup();
    let mint = Keypair::new();

    // This sets up the first owner's rate limit and the mint's hook metadata.
    setup_mint_and_extra_metas(&mut svm, &payer, &mint, &program_id);

    // Give a second owner SOL and initialize that owner's rate limit.
    let second_owner = Keypair::new();
    svm.airdrop(&second_owner.pubkey(), 1_000_000_000).unwrap();
    initialize_rate_limit(&mut svm, &second_owner, &mint, &program_id);

    let recipient = Keypair::new();
    svm.airdrop(&recipient.pubkey(), 1_000_000_000).unwrap();

    let first_source =
        create_ata(&mut svm, &payer, &payer.pubkey(), &mint.pubkey());
    let second_source =
        create_ata(&mut svm, &payer, &second_owner.pubkey(), &mint.pubkey());
    let destination =
        create_ata(&mut svm, &payer, &recipient.pubkey(), &mint.pubkey());

    // Fund both owners with enough tokens to each reach the limit.
    mint_tokens(&mut svm, &payer, &mint.pubkey(), &first_source, 1_000_000);
    mint_tokens(&mut svm, &payer, &mint.pubkey(), &second_source, 1_000_000);

    // The first owner transfers the full limit successfully.
    let first_ix = build_transfer_with_hook_ix(
        &first_source, &destination, &mint.pubkey(), &payer.pubkey(),
        &program_id, 1_000_000, 9,
    );
    let blockhash = svm.latest_blockhash();
    let message = Message::new_with_blockhash(
        &[first_ix], Some(&payer.pubkey()), &blockhash,
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(message), &[&payer],
    ).unwrap();
    assert!(
        svm.send_transaction(tx).is_ok(),
        "First owner's transfer should succeed"
    );

    // The second owner can also transfer the full limit in the same hour.
    let second_ix = build_transfer_with_hook_ix(
        &second_source, &destination, &mint.pubkey(), &second_owner.pubkey(),
        &program_id, 1_000_000, 9,
    );
    let blockhash = svm.latest_blockhash();
    let message = Message::new_with_blockhash(
        &[second_ix], Some(&second_owner.pubkey()), &blockhash,
    );
    let tx = VersionedTransaction::try_new(
        VersionedMessage::Legacy(message), &[&second_owner],
    ).unwrap();
    assert!(
        svm.send_transaction(tx).is_ok(),
        "Second owner's transfer should succeed independently"
    );
}
