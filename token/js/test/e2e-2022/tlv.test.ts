import chai, { expect } from 'chai';
import chaiAsPromised from 'chai-as-promised';
chai.use(chaiAsPromised);

import type { Connection, PublicKey, Signer } from '@solana/web3.js';
import { sendAndConfirmTransaction, Keypair, SystemProgram, Transaction } from '@solana/web3.js';
import {
    createAccount,
    createMint,
    createInitializeAccountInstruction,
    createEnableCpiGuardInstruction,
    getAccount,
    getCpiGuard,
    enableCpiGuard,
    disableCpiGuard,
    getAccountLen,
    ExtensionType,
    getExtensionData,
} from '../../src';
import { TEST_PROGRAM_ID, newAccountWithLamports, getConnection } from '../common';

// XXX OK WHAT THE FUCK AM I DOINGh
// i wanna proptest:
// * no extnsions
// * random permutations of extensions
// * with and without interspersal of uninitialized extensions
// * enablement of enableable extensions, including in possible uninitialized gaps
// i think memo and cpi are the only enableable (more correctly, jit initializable) ones
// i want to proptest the rust parser too because i dont really trust it so this will be good practice
//
// this means i need to map each extension to...
// * a closure to gen an instruction to create it
// * 
// wait fuck this cant be e2e its way too fucking slow
// but i dont have a way in js to generate proper buffer lawyouts
// hmm short term i can have it just fan out the chain calls
// longer term (ie if/when i write a rust proptest setup)...
// i can have a program that generates a fuckload of permutations and have js call that once
// and then we just test that it can parse them all
//
// ok fuck ANYWAY
// i dont need getters. fetch the account and call getExtensionData with each enum variant
// i need the jit inits amd the instruction builders
//
// oh i want my parse function to just go through and check for *every* extension
// and all the missing ones should return cleanly
const TEST_TOKEN_DECIMALS = 2;
describe('', () => {
    let connection: Connection;
    let payer: Signer;
    let owner: Keypair;

    let initTestMint: any;
    let initTestAccount: any;
    let extension_map: any = {};

    before(async () => {
        connection = await getConnection();
        payer = await newAccountWithLamports(connection, 1000000000);
        owner = Keypair.generate();

        extension_map[ExtensionType.CpiGuard] = {
            instruction: (account: PublicKey) => createEnableCpiGuardInstruction(account, owner.publicKey, [], TEST_PROGRAM_ID),
            initialize: async (account: PublicKey) => enableCpiGuard(connection, payer, account, owner, [], undefined, TEST_PROGRAM_ID),
        };

        initTestAccount = async (extensions: [ExtensionType]) => {
            const mintKeypair = Keypair.generate();
            const mintAuthority = Keypair.generate();
            const accountKeypair = Keypair.generate();
            const account = accountKeypair.publicKey;
            const accountLen = getAccountLen(extensions);
            const lamports = await connection.getMinimumBalanceForRentExemption(accountLen);

            const mint = await createMint(
                connection,
                payer,
                mintAuthority.publicKey,
                mintAuthority.publicKey,
                TEST_TOKEN_DECIMALS,
                mintKeypair,
                undefined,
                TEST_PROGRAM_ID
            );

            let transaction = new Transaction().add(
                SystemProgram.createAccount({
                    fromPubkey: payer.publicKey,
                    newAccountPubkey: account,
                    space: accountLen,
                    lamports,
                    programId: TEST_PROGRAM_ID,
                }),
                createInitializeAccountInstruction(account, mint, owner.publicKey, TEST_PROGRAM_ID),
            );
            for (let extension of extensions) {
                transaction.add(extension_map[extension].instruction(account));
            }

            let signers = [payer, accountKeypair];
            if (extensions.length > 0) {
                signers.push(owner);
            }

            await sendAndConfirmTransaction(connection, transaction, signers, undefined);

            return account;
        }
    });

    it('parse account, no extensions', async () => {
        const account = await initTestAccount([]);
    });

    it('HANA test whatever', async () => {
        // TODO gen perms here
        const extensions = [ExtensionType.CpiGuard];

        const account = await initTestAccount(extensions);
        const accountInfo = await getAccount(connection, account, undefined, TEST_PROGRAM_ID);

        // TODO check *all* extensions
        let cpiGuard = getExtensionData(ExtensionType.CpiGuard, accountInfo.tlvData);
        expect(cpiGuard).to.not.be.null;
    });

    // XXX OK NEXT when im back...
    // * write a simple function to generate permutations of extension type, make it fancy later
    // * write a function that takes types i expect to see and checks for presence/absence of all
    // * fix the actual bug in the parser? lol
    //   the problem is is gets five null bytes for uninitialized and trusts the length is zero
    //   check how the rust one does this i guess. but also i need to play around with the buffers
    //   i dont know whether uninitialized is always supposed to look... wait
    //   i actually dont know if... thats... ughhhhh i dont know why it *has* those bytes at all??
    //   oh wait yes i fucking do its because I TELL IT TO ALLOCATE THEM oh my god
    //   thats why the fucking memo transfer tests dont hit this, it inits it in one transaction
});
