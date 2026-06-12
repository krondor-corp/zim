/* tslint:disable */
/* eslint-disable */

/**
 * Result of [`encrypt_key_blob`]. Holds the values the hub needs to persist
 * for a viewer's identity-vault entry.
 */
export class KeyBlob {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly encryptedBlob: Uint8Array;
    readonly publicKey: Uint8Array;
    readonly salt: Uint8Array;
}

export class WasmVault {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Open a vault from the hub's decoded-manifest JSON. Requires a
     * session key (`loadKeyFromSession` / `unlockKeyBlob` first) —
     * the manifest share matching the session key's pubkey is
     * recovered into the vault's root secret.
     */
    static open(manifest_json: string): WasmVault;
    /**
     * Decrypt + decode a subdirectory body using the per-entry
     * secret its parent listed for it.
     */
    readDir(secret_hex: string, ciphertext: Uint8Array): string;
    /**
     * Decrypt a file body. Returns the plaintext bytes.
     *
     * File content uses the *streaming* cipher format (12-byte
     * nonce || raw ChaCha20 keystream — see `Secret::encrypt_reader`,
     * which `ContentStore::put_file` writes with), NOT the one-shot
     * AEAD envelope dir bodies use. Integrity comes from
     * content-addressing: the ciphertext hash was already verified
     * by fetching the blob at that hash.
     */
    readFile(secret_hex: string, ciphertext: Uint8Array): Uint8Array;
    /**
     * Decrypt + decode the root dir body. Returns a JSON array of
     * `{name, kind, hash, secret, mime}` entries.
     */
    readRootDir(ciphertext: Uint8Array): string;
    readonly height: bigint;
    readonly name: string;
    /**
     * blake3 hex of the encrypted root dir body. JS fetches this
     * via `/blob/{hash}` and hands the bytes to `readRootDir`.
     */
    readonly rootHash: string;
}

export function clearKey(): void;

export function decryptBlob(envelope_json: string, ciphertext: Uint8Array): Uint8Array;

/**
 * Encrypt the currently-loaded session key with a password-derived KEK and
 * return the artefacts the hub needs to store (`encrypted_blob`, `salt`,
 * `public_key`). The session key remains loaded.
 *
 * Errors if no session key is loaded (call [`generate_key`] or
 * [`unlock_key_blob`] first) or if randomness / KDF / AEAD fail.
 */
export function encryptKeyBlob(password: string): KeyBlob;

/**
 * Generate a fresh viewer keypair, store the secret in the session, return
 * the public key bytes for hub-side enrolment.
 */
export function generateKey(): Uint8Array;

export function init(): void;

export function loadKeyFromSession(key_bytes: Uint8Array): void;

/**
 * Hex of the session key's public key — lets JS match the right
 * share in a manifest without exporting the secret.
 */
export function publicKeyHex(): string;

/**
 * Sign a device-approval payload for the push-approval bootstrap flow.
 *
 * Produces an ed25519 signature over `pending_id || new_pubkey || expiry`
 * (big-endian u32 for expiry). The hub verifies this signature against the
 * approving device's pubkey before promoting the pending device.
 */
export function signApproval(pending_id: string, new_pubkey: Uint8Array, expiry_unix: number): Uint8Array;

/**
 * Sign an EdDSA JWT (compact JWS) using the session key.
 *
 * `claims_json` must be a JSON object containing at least `device_id`
 * (used as the `kid` in the JWT header). The returned string is the
 * compact serialization `base64url(header).base64url(payload).base64url(sig)`.
 */
export function signJwt(claims_json: string): string;

/**
 * Unlock a stored identity-vault blob with the viewer's password. On
 * success the recovered Ed25519 secret is loaded into the session and
 * [`decrypt_blob`] becomes usable for the `Sealed` envelope variant.
 *
 * Errors on wrong password (AEAD auth-tag mismatch), malformed blob, or
 * KDF/length issues.
 */
export function unlockKeyBlob(blob: Uint8Array, salt: Uint8Array, password: string): void;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_keyblob_free: (a: number, b: number) => void;
    readonly __wbg_wasmvault_free: (a: number, b: number) => void;
    readonly decryptBlob: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly encryptKeyBlob: (a: number, b: number) => [number, number, number];
    readonly generateKey: () => [number, number];
    readonly keyblob_encryptedBlob: (a: number) => [number, number];
    readonly keyblob_publicKey: (a: number) => [number, number];
    readonly keyblob_salt: (a: number) => [number, number];
    readonly loadKeyFromSession: (a: number, b: number) => [number, number];
    readonly publicKeyHex: () => [number, number, number, number];
    readonly signApproval: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly signJwt: (a: number, b: number) => [number, number, number, number];
    readonly unlockKeyBlob: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly wasmvault_height: (a: number) => bigint;
    readonly wasmvault_name: (a: number) => [number, number];
    readonly wasmvault_open: (a: number, b: number) => [number, number, number];
    readonly wasmvault_readDir: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly wasmvault_readFile: (a: number, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly wasmvault_readRootDir: (a: number, b: number, c: number) => [number, number, number, number];
    readonly wasmvault_rootHash: (a: number) => [number, number];
    readonly init: () => void;
    readonly clearKey: () => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
