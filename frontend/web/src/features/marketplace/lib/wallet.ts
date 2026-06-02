import { useState } from "react";

declare global {
  interface Window {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ethereum?: any;
  }
}

const STORAGE_KEY = "xvn_wallet_address";

export interface WalletState {
  address: string | null;
  connecting: boolean;
  connect: () => Promise<void>;
  disconnect: () => void;
}

export function useWallet(): WalletState {
  const [address, setAddress] = useState<string | null>(() =>
    localStorage.getItem(STORAGE_KEY),
  );
  const [connecting, setConnecting] = useState(false);

  async function connect(): Promise<void> {
    if (!window.ethereum) {
      throw new Error(
        "MetaMask (or compatible wallet) not detected. Install from metamask.io.",
      );
    }
    setConnecting(true);
    try {
      const accounts = (await window.ethereum.request({
        method: "eth_requestAccounts",
      })) as string[];
      const addr = accounts[0];
      localStorage.setItem(STORAGE_KEY, addr);
      setAddress(addr);
    } finally {
      setConnecting(false);
    }
  }

  function disconnect(): void {
    localStorage.removeItem(STORAGE_KEY);
    setAddress(null);
  }

  return { address, connecting, connect, disconnect };
}
