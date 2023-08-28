import { assertIsBase58EncodedAddress } from '@solana/web3.js';

export function ohai(address: any) {
    console.log('address', address);
    assertIsBase58EncodedAddress(address);
    console.log('worked!');
}
