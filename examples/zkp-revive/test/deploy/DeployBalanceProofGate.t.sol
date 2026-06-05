// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

interface BalanceProofGate {
    function submitProof(bytes memory proof) external payable returns (bool);
    function getPublicInputs() external view returns (uint256[4] memory);
}

function deployBalanceProofGate() external returns (address) {
    bytes32 codeHash = 0x6e3f7c99c328144e1c35f727f875f43b863e08dfe3d4d2775f1d3658b8b7b982;
    
    bytes memory proof = hex"00";
    
    vm.startBroadcast();
    new BalanceProofGate{value: 0, salt: keccak256(abi.encodePacked(codeHash, proof))}
        .submitProof(proof);
    vm.stopBroadcast();
    
    return address(0);
}