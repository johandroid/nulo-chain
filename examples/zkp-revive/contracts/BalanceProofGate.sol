// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface INoirVerifier {
    function verify(bytes calldata proof, bytes32[] calldata publicInputs)
        external
        view
        returns (bool);
}

contract BalanceProofGate {
    uint256 public constant DOMINANCE_PROOF = 0;
    uint256 public constant THRESHOLD_PROOF = 1;
    uint256 public constant PUBLIC_INPUTS = 4;

    INoirVerifier public immutable verifier;

    bytes32 public lastDominanceAccountCommitment;
    bytes32 public lastDominanceOtherCommitment;
    bytes32 public lastThresholdAccountCommitment;
    bytes32 public lastThreshold;

    event AccountDominanceProven(bytes32 indexed accountCommitment, bytes32 indexed otherCommitment);
    event ThresholdProven(bytes32 indexed accountCommitment, bytes32 threshold);

    constructor(address verifierAddress) {
        verifier = INoirVerifier(verifierAddress);
    }

    function submitDominanceProof(bytes calldata proof, bytes32[] calldata publicInputs)
        external
        returns (bool)
    {
        require(publicInputs.length == PUBLIC_INPUTS, "bad public input count");
        require(uint256(publicInputs[0]) == DOMINANCE_PROOF, "wrong proof kind");
        require(verifier.verify(proof, publicInputs), "invalid proof");

        bytes32 accountCommitment = publicInputs[2];
        bytes32 otherCommitment = publicInputs[3];

        lastDominanceAccountCommitment = accountCommitment;
        lastDominanceOtherCommitment = otherCommitment;

        emit AccountDominanceProven(accountCommitment, otherCommitment);
        return true;
    }

    function submitThresholdProof(bytes calldata proof, bytes32[] calldata publicInputs)
        external
        returns (bool)
    {
        require(publicInputs.length == PUBLIC_INPUTS, "bad public input count");
        require(uint256(publicInputs[0]) == THRESHOLD_PROOF, "wrong proof kind");
        require(verifier.verify(proof, publicInputs), "invalid proof");

        bytes32 accountCommitment = publicInputs[2];
        bytes32 threshold = publicInputs[1];

        lastThresholdAccountCommitment = accountCommitment;
        lastThreshold = threshold;

        emit ThresholdProven(accountCommitment, threshold);
        return true;
    }
}
