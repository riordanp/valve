import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { PublicKey } from "@solana/web3.js";
import { assert, expect } from "chai";
import { Example } from "../target/types/example";
import { Valve } from "../target/types/valve";

describe("valve", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const valve = anchor.workspace.Valve as Program<Valve>;
  const example = anchor.workspace.Example as Program<Example>;

  const endpoint = 111;
  const maxReqs = 5;
  let policyPDA: PublicKey;
  let bucketPDA: PublicKey;

  async function makeCheckIx() {
    return await valve.methods
      .check()
      .accounts({
        bucket: bucketPDA,
        policy: policyPDA,
        owner: provider.wallet.publicKey,
      })
      .instruction();
  }

  async function makeCallIx(
    program: PublicKey,
    innerInstructionData: Buffer,
    remainingAccounts: anchor.web3.AccountMeta[]
  ) {
    return await valve.methods
      .call(innerInstructionData)
      .accounts({
        bucket: bucketPDA,
        policy: policyPDA,
        owner: provider.wallet.publicKey,
      })
      .remainingAccounts(
        [{ pubkey: program, isSigner: false, isWritable: false }].concat(
          remainingAccounts
        )
      )
      .instruction();
  }

  async function makeTestIx(a: number = 1, b: number = 1) {
    return await example.methods
      .test(a, b)
      .accounts({ instructions: anchor.web3.SYSVAR_INSTRUCTIONS_PUBKEY })
      .instruction();
  }

  async function makeTestCpiIx(a: number = 1, b: number = 1) {
    return await example.methods.testCpi(a, b).instruction();
  }

  before(async () => {
    // Create policy if not exists
    const endpointBytes = Buffer.alloc(4);
    endpointBytes.writeUInt32LE(endpoint);
    policyPDA = (
      await PublicKey.findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("Policy"),
          example.programId.toBuffer(),
          endpointBytes,
        ],
        valve.programId
      )
    )[0];

    try {
      await valve.account.policy.fetch(policyPDA);
    } catch (e) {
      const tx = await valve.methods
        .initializePolicy(endpoint, maxReqs, 60)
        .accounts({
          payer: provider.wallet.publicKey,
          program: example.programId,
        })
        .rpc();

      console.log("initializePolicy", policyPDA.toBase58(), tx);
    }

    // Create bucket if not exists
    bucketPDA = (
      await PublicKey.findProgramAddress(
        [
          anchor.utils.bytes.utf8.encode("Bucket"),
          policyPDA.toBuffer(),
          provider.wallet.publicKey.toBuffer(),
        ],
        valve.programId
      )
    )[0];

    try {
      await valve.account.bucket.fetch(bucketPDA);
    } catch (e) {
      const tx = await valve.methods
        .initializeBucket()
        .accounts({
          owner: provider.wallet.publicKey,
          policy: policyPDA,
        })
        .rpc();
      console.log("initializeBucket", bucketPDA.toBase58(), tx);
    }
  });
  
  it("allows a checked call", async () => {
    const checkedTx = new anchor.web3.Transaction()
      .add(await makeCheckIx())
      .add(await makeTestIx());
    let checkedError = false;
    try {
      await provider.sendAndConfirm(checkedTx);
    } catch (e) {
      console.error(e);
      checkedError = true;
    }
    assert(!checkedError, "checked should have succeeded");
    await sleep(12500);
  });

  it("disallows an unchecked call", async () => {
    const tx = new anchor.web3.Transaction().add(await makeTestIx());
    let error = false;
    try {
      await provider.sendAndConfirm(tx);
    } catch {
      error = true;
    }
    assert(error, "unchecked should have failed");
    await sleep(12500);
  });

  it("fails transactions that exceed the rate limit", async () => {
    let errors = 0;
    for (let i = 0; i < maxReqs * 2; i++) {
      const tx = new anchor.web3.Transaction()
        .add(await makeCheckIx())
        .add(await makeTestIx(i));
      try {
        await provider.sendAndConfirm(tx);
      } catch (e) {
        errors++;
      }
    }
    expect(errors).to.be.greaterThanOrEqual(maxReqs);
    await sleep(12500);
  });

  it("allows a cpi call", async () => {
    const tx = new anchor.web3.Transaction().add(
      await makeCallIx(example.programId, (await makeTestCpiIx()).data, [])
    );
    try {
      await provider.sendAndConfirm(tx);
    } catch (e) {
      console.error(e);
      throw e;
    }
    await sleep(12500);
  });

  it("fails cpi calls that exceed the rate limit", async () => {
    let errors = 0;
    for (let i = 0; i < maxReqs * 2; i++) {
      const tx = new anchor.web3.Transaction().add(
        await makeCallIx(example.programId, (await makeTestCpiIx()).data, [])
      );
      try {
        await provider.sendAndConfirm(tx);
      } catch (e) {
        errors++;
      }
    }
    expect(errors).to.be.greaterThanOrEqual(maxReqs);
    await sleep(12500);
  });

  it("refills tokens after exhaustion", async () => {
    for (let i = 0; i < maxReqs * 2; i++) {
      const tx = new anchor.web3.Transaction()
        .add(await makeCheckIx())
        .add(await makeTestIx(i));
      try {
        await provider.sendAndConfirm(tx);
      } catch {}
    }

    // wait for at least one token to refill
    await sleep(12500);
    let error = false;
    const tx = new anchor.web3.Transaction()
      .add(await makeCheckIx())
      .add(await makeTestIx());
    try {
      await provider.sendAndConfirm(tx);
    } catch {
      error = true;
    }
    expect(!error);
    await sleep(12500);
  });
});

async function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
