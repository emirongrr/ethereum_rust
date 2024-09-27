// SPDX-License-Identifier: MIT
pragma solidity 0.8.27;

import "./interfaces/IInbox.sol";

/// @title Inbox contract.
/// @author LambdaClass
contract Inbox is IInbox {
    /// @inheritdoc IInbox
    function deposit(address /*to*/, address /*refundRecipient*/) external payable {
        // TODO: Build the tx.
        bytes32 l2MintTxHash = keccak256(abi.encode());
        emit DepositInitiated(l2MintTxHash);
    }
}