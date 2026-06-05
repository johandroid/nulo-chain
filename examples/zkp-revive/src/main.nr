global DOMINANCE_PROOF: u8 = 0;
global THRESHOLD_PROOF: u8 = 1;

fn balance_commitment(balance: u64, salt: Field) -> Field {
    std::hash::pedersen_hash([balance as Field, salt])
}

fn main(
    proof_kind: pub u8,
    threshold: pub u64,
    account_balance: u64,
    account_salt: Field,
    other_balance: u64,
    other_salt: Field,
) -> pub [Field; 2] {
    let account_commitment = balance_commitment(account_balance, account_salt);
    let other_commitment = balance_commitment(other_balance, other_salt);

    if proof_kind == DOMINANCE_PROOF {
        assert(account_balance > other_balance);
    } else {
        assert(proof_kind == THRESHOLD_PROOF);
        assert(account_balance > threshold);
    }

    [account_commitment, other_commitment]
}
