import chai, { expect } from 'chai';
import chaiAsPromised from 'chai-as-promised';
chai.use(chaiAsPromised);

import type { Connection, PublicKey, Signer } from '@solana/web3.js';
import { sendAndConfirmTransaction, Keypair, SystemProgram, Transaction } from '@solana/web3.js';

import type { Account } from '../../src';
import {
    createAccount,
    createMint,
    createInitializeAccountInstruction,
    createEnableCpiGuardInstruction,
    createEnableRequiredMemoTransfersInstruction,
    createInitializeTransferFeeConfigInstruction,
    getAccount,
    getCpiGuard,
    enableCpiGuard,
    enableRequiredMemoTransfers,
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
// * extra bytes at the end
// * the shit that it uses to make mints/accounts not look like multisigs
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

const MINT_EXTENSIONS = [
    ExtensionType.TransferFeeConfig,
    ExtensionType.MintCloseAuthority,
    ExtensionType.ConfidentialTransferMint,
    ExtensionType.DefaultAccountState,
    ExtensionType.NonTransferable,
    ExtensionType.InterestBearingConfig,
    ExtensionType.PermanentDelegate,
];

const ACCOUNT_EXTENSIONS = [
    ExtensionType.TransferFeeAmount,
    ExtensionType.ConfidentialTransferAccount,
    ExtensionType.ImmutableOwner,
    ExtensionType.MemoTransfer,
    ExtensionType.CpiGuard,
];

// we always choose at least one because we have separate tests for zero
function chooseExtensions(extensions: ExtensionType[]) {
    extensions = extensions.slice();

    // TODO remove this once confidential support lands
    extensions = extensions.filter(e => e != ExtensionType.ConfidentialTransferMint && e != ExtensionType.ConfidentialTransferAccount);

    // TODO ill add immutable instruction myself lol
    extensions = extensions.filter(e => e != ExtensionType.ImmutableOwner);

    // TODO lmao i need to write a function to init mints with extensions...
    extensions = extensions.filter(e => e != ExtensionType.TransferFeeAmount);

    for(let i = extensions.length - 1; i > 0; i--) {
        const j = Math.floor(Math.random() * (i + 1));
        [extensions[i], extensions[j]] = [extensions[j], extensions[i]];
    }

    return extensions.slice(Math.floor(Math.random() * extensions.length));
}

describe('', () => {
    let connection: Connection;
    let payer: Signer;
    let owner: Keypair;

    let initTestMint: Function;
    let initTestAccount: Function;
    let extension_map: any = {};

    before(async () => {
        connection = await getConnection();
        payer = await newAccountWithLamports(connection, 1000000000);
        owner = Keypair.generate();

        extension_map[ExtensionType.CpiGuard] = {
            instruction: (account: PublicKey) =>
                createEnableCpiGuardInstruction(account, owner.publicKey, [], TEST_PROGRAM_ID),
            initialize: async (account: PublicKey) =>
                enableCpiGuard(connection, payer, account, owner, [], undefined, TEST_PROGRAM_ID),
            signer: owner,
        };

        extension_map[ExtensionType.MemoTransfer] = {
            instruction: (account: PublicKey) =>
                createEnableRequiredMemoTransfersInstruction(account, owner.publicKey, [], TEST_PROGRAM_ID),
            initialize: async (account: PublicKey) =>
                enableRequiredMemoTransfers(connection, payer, account, owner, [], undefined, TEST_PROGRAM_ID),
            signer: owner,
        };

        // account extension is automatically enforced by mint extension
        extension_map[ExtensionType.TransferFeeAmount] = {};
        extension_map[ExtensionType.TransferFeeConfig] = {
            instruction: (mint: PublicKey) =>
                createInitializeTransferFeeConfigInstruction(mint, null, null, 1, 10n, TEST_PROGRAM_ID),
        };

        initTestAccount = async (extensions: ExtensionType[] = [], extraSpace: number = 0) => {
            const mintKeypair = Keypair.generate();
            const mintAuthority = Keypair.generate();
            const accountKeypair = Keypair.generate();
            const account = accountKeypair.publicKey;
            const accountLen = getAccountLen(extensions) + extraSpace;
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

            let signers = [payer, accountKeypair];
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
                let ext = extension_map[extension]
                transaction.add(ext.instruction(account));
                if (ext.signer && !signers.includes(ext.signer)) signers.push(ext.signer);
            }

            await sendAndConfirmTransaction(connection, transaction, signers, {skipPreflight: true});

            return account;
        }
    });

    // this makes sure we cover all possible extensions
    // if youre reading this because youre trying to add an extension and this test failed
    // please add its ExtensionType value to MINT_EXTENSIONS or ACCOUNT_EXTENSIONS as appropriate
    // and add an entry for it to extension_map
    it('extensions are exhaustive', () => {
        let our_extensions = [ExtensionType.Uninitialized].concat(MINT_EXTENSIONS, ACCOUNT_EXTENSIONS);
        let their_extensions = Object.values(ExtensionType).filter((v: any) => !isNaN(v));

        expect(our_extensions.sort()).to.eql(their_extensions.sort());
    });

    // test that the parser gracefully handles accounts with arbitrary extra space
    it('parse account, no extensions', async () => {
        let promises = [];

        for(let i = 0; i < 16; i++) {
            // trying to alloc exactly one extra byte causes an unpack failure in the program when initializing
            if (i == 1) continue;

            promises.push(
                initTestAccount([], i)
                .then((account: PublicKey) => getAccount(connection, account, undefined, TEST_PROGRAM_ID))
                .then((accountInfo: Account) => [i, accountInfo])
            );
        }

        for (let promise of promises) {
            let [bytes, accountInfo] = await promise;
            for (let extension of ACCOUNT_EXTENSIONS) {
                expect(
                    getExtensionData(extension, accountInfo.tlvData),
                    `account parse test failed. test case: no extensions, ${bytes} extra bytes` 
                ).to.be.null;
            }
        }
    });

    it('HANA test whatever new', async () => {
        for(let i = 0; i < 20; i++) {
            console.log(chooseExtensions(ACCOUNT_EXTENSIONS));
        }
    });

/*
    it('HANA test whatever', async () => {
        // TODO gen perms here
        const extensions = [ExtensionType.CpiGuard];

        const account = await initTestAccount(extensions);
        const accountInfo = await getAccount(connection, account, undefined, TEST_PROGRAM_ID);

        let rawAccount = await connection.getAccountInfo(account);
        console.log("HANA ai:", rawAccount!.data!.toString('hex')!.match(/../g)!.join(' '));
        console.log("     len:", rawAccount!.data!.length);

        // TODO check *all* extensions
        let cpiGuard = getExtensionData(ExtensionType.CpiGuard, accountInfo.tlvData);
        expect(cpiGuard).to.not.be.null;
    });
*/

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
    //
    // ok cool testing just empty space on accounts works good. next thing i need to do iy
    // * gen extension combinations, possibly interspersed with uninits
    //   wait can you actually "init" an uninit this might just be not possible
    //   note i need to use getAccountTypeOfMintType backwards to see if i need to init shit on the mint
    // * 
    // * init accounts with those extensions
    //
    // XXX OK lol this is ind of annoying, when i come back i need to
    // * write my init test mint fn because theres no token22 mint init
    // * write an immutable owner instruction function
    // * write the logic to init transfer fee config if the account needs to have transfer fee account
});
