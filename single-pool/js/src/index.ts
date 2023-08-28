import { Keypair } from '@solana/web3.js';
import { ohai as ohaiModern } from "single-pool-next";

export function ohai() {
    const keyPair = Keypair.generate();
    console.log('keyPair', keyPair);
    ohaiModern(keyPair.publicKey.toBase58());
}

ohai();
