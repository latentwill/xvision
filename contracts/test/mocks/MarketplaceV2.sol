// SPDX-License-Identifier: Apache-2.0
pragma solidity 0.8.24;

import {Marketplace} from "../../src/Marketplace.sol";

/// @title MarketplaceV2 — a trivial upgrade target for UUPS upgrade-safety
///        tests. Adds a view function without touching the existing storage
///        layout (any new state would go into the reserved `__gap`).
contract MarketplaceV2 is Marketplace {
    function version() external pure returns (string memory) {
        return "v2";
    }
}
