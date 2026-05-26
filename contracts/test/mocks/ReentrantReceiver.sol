// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {IERC1155Receiver} from "@openzeppelin/contracts/token/ERC1155/IERC1155Receiver.sol";
import {IERC165} from "@openzeppelin/contracts/utils/introspection/IERC165.sol";
import {IMarketplace} from "../../src/interfaces/IMarketplace.sol";

/// @title ReentrantReceiver — attempts to re-enter `Marketplace.buy` from the
///        ERC-1155 mint acceptance hook.
/// @dev Used to prove the `nonReentrant` guard holds: when `Marketplace` mints
///      the license to this contract, `onERC1155Received` fires and calls
///      `buy` again. The reentrant call must revert, which bubbles up and
///      reverts the whole purchase.
contract ReentrantReceiver is IERC1155Receiver {
    IMarketplace public immutable marketplace;
    uint256 public immutable targetListingId;
    bool public reentered;

    constructor(IMarketplace marketplace_, uint256 targetListingId_) {
        marketplace = marketplace_;
        targetListingId = targetListingId_;
    }

    function onERC1155Received(address, address, uint256, uint256, bytes calldata)
        external
        returns (bytes4)
    {
        reentered = true;
        // Re-enter — expected to revert under the nonReentrant guard.
        marketplace.buy(targetListingId, address(this));
        return this.onERC1155Received.selector;
    }

    function onERC1155BatchReceived(
        address,
        address,
        uint256[] calldata,
        uint256[] calldata,
        bytes calldata
    ) external pure returns (bytes4) {
        return this.onERC1155BatchReceived.selector;
    }

    function supportsInterface(bytes4 interfaceId) external pure returns (bool) {
        return interfaceId == type(IERC1155Receiver).interfaceId
            || interfaceId == type(IERC165).interfaceId;
    }
}
