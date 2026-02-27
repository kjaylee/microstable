    const CFG = {
      RPC_URLS: [
        "https://api.devnet.solana.com",
        "https://devnet.rpcpool.com"
      ],
      RPC_URL: "https://api.devnet.solana.com",
      EXPECTED_GENESIS_HASH: "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
      PROGRAM_ID: "BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3",
      MSTB_MINT: "EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R",
      PROTOCOL_STATE: "9NbeDUSPdhC4ZgpefoqT3p48eLEyXknQJEm6v5pLGFQP",
      CIRCUIT_BREAKER: "7xy7xc4nqhywYa72Bb5A2u7g3t6kz96HN2e2z4Yn9WXe",
      TOKEN_PROGRAM: "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA",
      ASSOCIATED_TOKEN_PROGRAM: "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL",
      FEEDS: {
        USDC: "Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX",
        USDT: "HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM",
        DAI: "FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN"
      },
      TICK_MS: 30000,
      HISTORY_KEY: "microstable:param-history:v1",
      SCORE_KEY: "microstable:score-history:v1"
    };

    const ROLE_MAP = ["Optimizer", "Monitor", "Auditor", "Liquidator"];
    const AGENT_STATUS = ["Active", "Cooldown", "Slashed", "Deregistered"];
    const COLLATERAL_META = {
      0: { symbol: "USDC", decimals: 6 },
      1: { symbol: "USDT", decimals: 6 },
      2: { symbol: "DAI", decimals: 18 },
      3: { symbol: "USDS", decimals: 18 }
    };
    const AGENT_ROLE_MIN_STAKE_SOL = {
      0: 10,
      1: 5,
      2: 20,
      3: 2
    };

    const ACCOUNT_DISCRIMINATORS = {
      AGENT_RECORD: new Uint8Array([4, 201, 129, 70, 197, 134, 47, 169])
    };

    const FAUCET_CONFIG = {
      instructionAvailable: true,
      // DEVNET ONLY — mint authority keypair for test collateral tokens (zero real value)
      faucetKeypair: [200,216,244,99,87,136,60,121,147,67,211,155,111,98,248,181,119,4,110,112,50,204,105,10,156,77,154,44,6,164,12,32,39,162,172,128,64,158,200,20,181,231,32,205,219,37,123,128,72,159,230,201,207,196,85,88,109,182,30,117,48,196,103,3],
      faucetAmounts: { 0: 1000_000_000, 1: 1000_000_000, 2: 10_000_000_000_000_000_000n }, // 1000 USDC (6d), 1000 USDT (6d), 10 DAI (18d, u64-safe)
      hint: "Devnet faucet ready"
    };

    const state = {
      lastSuccessAt: 0,
      lastKeeperActivity: 0,
      polling: false,
      history: loadHistory(),
      prevScores: loadJson(CFG.SCORE_KEY, {}),
      prev: {},
      prevWeights: [],
      prevAgents: {},
      prevTxSigs: new Set(),
      prevOracles: {},
      agentRows: new Map(),
      txRows: new Map(),
      oracleCardsReady: false,
      weightRowsReady: false,
      lastProtocol: null,
      lastVaults: [],
      lastAgents: [],
      collateralMints: {},
      lastError: "",
      connection: null,
      pubkeys: null,
      wallet: {
        provider: null,
        installed: false,
        publicKey: null,
        balances: { USDC: 0, USDT: 0, DAI: 0, MSTB: 0 },
        balanceBusy: false
      },
      mintBusy: false,
      redeemBusy: false,
      registerBusy: false,
      instructionDiscriminators: {},
      faucet: {
        mintAuthorities: {},
        lastMintsKey: "",
        checked: false,
        checking: false,
        airdropBusy: false
      },
      rpcCursor: 0,
      rpcBootstrapDone: false
    };

    const $ = (id) => document.getElementById(id);
    const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

    function shortKey(v) {
      if (!v || v.length < 12) return v || "--";
      return `${v.slice(0, 6)}...${v.slice(-6)}`;
    }

    function loadJson(key, fallback) {
      try {
        const raw = localStorage.getItem(key);
        if (!raw) return fallback;
        return JSON.parse(raw);
      } catch {
        return fallback;
      }
    }

    function saveJson(key, value) {
      try { localStorage.setItem(key, JSON.stringify(value)); } catch {}
    }

    function loadHistory() {
      const arr = loadJson(CFG.HISTORY_KEY, []);
      return Array.isArray(arr) ? arr : [];
    }

    function fmtAgoMs(ms) {
      if (!ms) return "--";
      const sec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
      if (sec < 60) return `${sec}s`;
      const min = Math.floor(sec / 60);
      if (min < 60) return `${min}m ${sec % 60}s`;
      const h = Math.floor(min / 60);
      return `${h}h ${min % 60}m`;
    }

    function fmtAgoUnix(ts) {
      if (!ts) return "--";
      const sec = Math.max(0, Math.floor(Date.now() / 1000) - Number(ts));
      if (sec < 60) return `${sec}s ago`;
      const min = Math.floor(sec / 60);
      if (min < 60) return `${min}m ago`;
      const h = Math.floor(min / 60);
      return `${h}h ${min % 60}m ago`;
    }

    function fmtClock(ts) {
      if (!ts) return "--";
      return new Date(Number(ts) * 1000).toLocaleTimeString("en-GB", { hour12: false });
    }

    function fmtToken(raw, decimals = 6, maxFrac = 2) {
      if (raw == null) return "--";
      const n = Number(raw) / Math.pow(10, decimals);
      if (!Number.isFinite(n)) return "--";
      return n.toLocaleString(undefined, { maximumFractionDigits: maxFrac });
    }

    function ppmToPct(ppm) {
      return Number(ppm) / 10000;
    }

    function animateNumber(el, next, opts = {}) {
      const decimals = opts.decimals ?? 2;
      const suffix = opts.suffix ?? "";
      const duration = opts.duration ?? 500;
      const prev = Number(el.dataset.v || next);
      const target = Number(next);
      el.dataset.v = String(target);
      const t0 = performance.now();
      const tick = (t) => {
        const p = Math.min(1, (t - t0) / duration);
        const e = 1 - Math.pow(1 - p, 3);
        const now = prev + (target - prev) * e;
        el.textContent = `${now.toFixed(decimals)}${suffix}`;
        if (p < 1) requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    }

    function flashOnce(el) {
      if (!el) return;
      el.classList.remove("flash");
      void el.offsetWidth;
      el.classList.add("flash");
      setTimeout(() => el.classList.remove("flash"), 600);
    }

    function setIfChanged(el, text) {
      if (!el) return false;
      if (el.textContent !== text) {
        el.textContent = text;
        flashOnce(el);
        return true;
      }
      return false;
    }

    function setTextOnlyIfChanged(el, text) {
      if (!el) return false;
      if (el.textContent !== text) {
        el.textContent = text;
        return true;
      }
      return false;
    }

    function rpcEndpoints() {
      const list = Array.isArray(CFG.RPC_URLS) ? CFG.RPC_URLS.filter(Boolean) : [];
      return list.length ? list : [CFG.RPC_URL];
    }

    function shouldCrossCheck(method) {
      // Dynamic devnet endpoints can legitimately differ by context slot/order.
      // Keep bootstrap genesis validation strict, but skip runtime quorum checks
      // to avoid false negatives that force the dashboard offline.
      return method === "getGenesisHash";
    }

    function stableRpcResultKey(result) {
      return JSON.stringify(result);
    }

    async function rpcRequest(url, payload, timeout) {
      const controller = new AbortController();
      const timer = setTimeout(() => controller.abort(), timeout);
      try {
        const res = await fetch(url, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
          signal: controller.signal,
          cache: "no-store"
        });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = await res.json();
        if (data.error) throw new Error(data.error.message || "RPC error");
        return data.result;
      } finally {
        clearTimeout(timer);
      }
    }

    async function verifyRpcBootstrap() {
      if (state.rpcBootstrapDone) return;
      const endpoints = rpcEndpoints();
      const verified = [];
      const probePayload = { jsonrpc: "2.0", id: Math.floor(Math.random() * 1e9), method: "getGenesisHash", params: [] };

      for (const endpoint of endpoints) {
        try {
          const genesisHash = await rpcRequest(endpoint, probePayload, 7000);
          if (genesisHash !== CFG.EXPECTED_GENESIS_HASH) {
            throw new Error(`genesis hash mismatch (${genesisHash})`);
          }
          verified.push(endpoint);
        } catch (err) {
          console.warn(`RPC bootstrap probe failed for ${endpoint}:`, err?.message || err);
        }
      }

      if (!verified.length) {
        throw new Error("RPC bootstrap failed: no endpoint passed genesis verification");
      }
      if (verified.length < endpoints.length) {
        console.warn(
          `RPC bootstrap degraded: ${verified.length}/${endpoints.length} endpoints passed genesis verification`
        );
      }

      CFG.RPC_URLS = verified;
      CFG.RPC_URL = verified[0];
      state.rpcBootstrapDone = true;
    }

    async function crossCheckRpcQuorum(endpoints, startIdx, payload, timeout, primaryResult) {
      const observations = [{ endpoint: endpoints[startIdx], result: primaryResult }];
      const maxChecks = Math.min(3, endpoints.length);
      const peerErrors = [];

      for (let offset = 1; offset < maxChecks; offset++) {
        const endpoint = endpoints[(startIdx + offset) % endpoints.length];
        try {
          const result = await rpcRequest(endpoint, payload, timeout);
          observations.push({ endpoint, result });
        } catch (err) {
          peerErrors.push(`${endpoint}: ${err?.message || String(err)}`);
        }
      }

      const bucketMap = new Map();
      for (const sample of observations) {
        const key = stableRpcResultKey(sample.result);
        const bucket = bucketMap.get(key) || {
          count: 0,
          result: sample.result,
          endpoints: []
        };
        bucket.count += 1;
        bucket.endpoints.push(sample.endpoint);
        bucketMap.set(key, bucket);
      }

      let best = null;
      for (const bucket of bucketMap.values()) {
        if (!best || bucket.count > best.count) best = bucket;
      }

      const required = observations.length > 1 ? 2 : 1;
      if (!best || best.count < required) {
        const observed = observations.map((o) => o.endpoint).join(", ");
        throw new Error(`cross-RPC quorum mismatch across endpoints: ${observed}`);
      }

      if (peerErrors.length) {
        console.warn(`cross-RPC quorum degraded: ${peerErrors.join(" | ")}`);
      }

      return best.result;
    }

    async function rpc(method, params, timeout = 14000) {
      await verifyRpcBootstrap();

      const endpoints = rpcEndpoints();
      const payload = { jsonrpc: "2.0", id: Math.floor(Math.random() * 1e9), method, params };
      const errors = [];

      for (let i = 0; i < endpoints.length; i++) {
        const idx = (state.rpcCursor + i) % endpoints.length;
        const endpoint = endpoints[idx];

        try {
          const primary = await rpcRequest(endpoint, payload, timeout);
          const result = (endpoints.length > 1 && shouldCrossCheck(method))
            ? await crossCheckRpcQuorum(endpoints, idx, payload, timeout, primary)
            : primary;

          state.rpcCursor = idx;
          CFG.RPC_URL = endpoint;
          return result;
        } catch (err) {
          errors.push(`${endpoint}: ${err?.message || String(err)}`);
        }
      }

      throw new Error(`RPC request failed (${method}): ${errors.join(" | ")}`);
    }

    function base64ToBytes(b64) {
      const bin = atob(b64);
      const out = new Uint8Array(bin.length);
      for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
      return out;
    }

    function hasDiscriminator(bytes, discriminator) {
      if (!bytes || !discriminator || bytes.length < discriminator.length) return false;
      for (let i = 0; i < discriminator.length; i++) {
        if (bytes[i] !== discriminator[i]) return false;
      }
      return true;
    }

    function readU32LE(dv, o) { return dv.getUint32(o, true); }
    function readI32LE(dv, o) { return dv.getInt32(o, true); }
    function readU64LE(dv, o) {
      const lo = BigInt(dv.getUint32(o, true));
      const hi = BigInt(dv.getUint32(o + 4, true));
      return (hi << 32n) | lo;
    }
    function readI64LE(dv, o) {
      const u = readU64LE(dv, o);
      const max = 1n << 63n;
      return u >= max ? u - (1n << 64n) : u;
    }

    const BASE58_ALPHABET = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    function bytesToBase58(bytes) {
      if (!bytes || bytes.length === 0) return "";
      let digits = [0];
      for (let i = 0; i < bytes.length; i++) {
        let carry = bytes[i];
        for (let j = 0; j < digits.length; j++) {
          carry += digits[j] << 8;
          digits[j] = carry % 58;
          carry = (carry / 58) | 0;
        }
        while (carry > 0) {
          digits.push(carry % 58);
          carry = (carry / 58) | 0;
        }
      }
      let leadingZeroes = 0;
      while (leadingZeroes < bytes.length && bytes[leadingZeroes] === 0) leadingZeroes++;
      let out = "";
      for (let i = 0; i < leadingZeroes; i++) out += "1";
      for (let i = digits.length - 1; i >= 0; i--) out += BASE58_ALPHABET[digits[i]];
      return out;
    }

    function readPubkey(bytes, o) {
      return bytesToBase58(bytes.slice(o, o + 32));
    }

    function parseProtocolState(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 338) throw new Error("ProtocolState too short");
      let o = 8;
      const weights = [];
      for (let i = 0; i < 4; i++) { weights.push(readU64LE(dv, o)); o += 8; }
      const fee_rate = readU64LE(dv, o); o += 8;
      const mint_fee_rate = readU64LE(dv, o); o += 8;
      const redeem_fee_rate = readU64LE(dv, o); o += 8;
      const cr_target = readU64LE(dv, o); o += 8;
      const total_supply = readU64LE(dv, o); o += 8;
      const last_update_slot = readU64LE(dv, o); o += 8;
      const keeper_set = [];
      for (let i = 0; i < 3; i++) { keeper_set.push(readPubkey(bytes, o)); o += 32; }
      const emergency_shutdown = !!bytes[o]; o += 1;
      const pending_rebalance_commit = bytes.slice(o, o + 32); o += 32;
      const pending_rebalance_slot = readU64LE(dv, o); o += 8;
      const pending_rebalance_expiry = readU64LE(dv, o); o += 8;
      const pending_keeper_set = [];
      for (let i = 0; i < 3; i++) { pending_keeper_set.push(readPubkey(bytes, o)); o += 32; }
      const pending_keeper_activation_slot = readU64LE(dv, o); o += 8;
      const flow_control_slot = readU64LE(dv, o); o += 8;
      const minted_in_flow_slot = readU64LE(dv, o); o += 8;
      const redeemed_in_flow_slot = readU64LE(dv, o); o += 8;
      const max_mint_per_slot_ppm = readU64LE(dv, o); o += 8;
      const max_redeem_per_slot_ppm = readU64LE(dv, o); o += 8;
      const manual_oracle_mode_expiry_slot = readU64LE(dv, o); o += 8;
      const bump = bytes[o]; o += 1;

      let manual_oracle_reenable_delay_slots = 0n;
      let manual_oracle_last_activation_slot = 0n;
      let manual_oracle_activation_epoch = 0n;
      let manual_oracle_activation_count_epoch = 0n;
      if (bytes.length >= o + 32) {
        manual_oracle_reenable_delay_slots = readU64LE(dv, o); o += 8;
        manual_oracle_last_activation_slot = readU64LE(dv, o); o += 8;
        manual_oracle_activation_epoch = readU64LE(dv, o); o += 8;
        manual_oracle_activation_count_epoch = readU64LE(dv, o); o += 8;
      }

      const EMPTY_PUBKEY = "11111111111111111111111111111111";
      let usdc_mint = null;
      let usdt_mint = null;
      let dai_mint = null;
      let usds_mint = null;
      if (bytes.length >= o + 128) {
        usdc_mint = readPubkey(bytes, o); o += 32;
        usdt_mint = readPubkey(bytes, o); o += 32;
        dai_mint = readPubkey(bytes, o); o += 32;
        usds_mint = readPubkey(bytes, o); o += 32;

        if (usdc_mint === EMPTY_PUBKEY) usdc_mint = null;
        if (usdt_mint === EMPTY_PUBKEY) usdt_mint = null;
        if (dai_mint === EMPTY_PUBKEY) dai_mint = null;
        if (usds_mint === EMPTY_PUBKEY) usds_mint = null;
      }

      return {
        weights, fee_rate, mint_fee_rate, redeem_fee_rate, cr_target, total_supply,
        last_update_slot, keeper_set, emergency_shutdown, pending_rebalance_commit,
        pending_rebalance_slot, pending_rebalance_expiry, pending_keeper_set,
        pending_keeper_activation_slot, flow_control_slot, minted_in_flow_slot,
        redeemed_in_flow_slot, max_mint_per_slot_ppm, max_redeem_per_slot_ppm,
        manual_oracle_mode_expiry_slot, bump,
        manual_oracle_reenable_delay_slots,
        manual_oracle_last_activation_slot,
        manual_oracle_activation_epoch,
        manual_oracle_activation_count_epoch,
        usdc_mint, usdt_mint, dai_mint, usds_mint
      };
    }

    function parseCircuitBreaker(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 203) throw new Error("CircuitBreaker too short");
      let o = 8;
      const status = Array.from(bytes.slice(o, o + 4)); o += 4;
      const activation_tick = [];
      const trigger_count = [];
      const cooldown_until = [];
      const last_trigger_tick = [];
      const recovery_tick = [];
      for (let i = 0; i < 4; i++) { activation_tick.push(readU64LE(dv, o)); o += 8; }
      for (let i = 0; i < 4; i++) { trigger_count.push(readU64LE(dv, o)); o += 8; }
      for (let i = 0; i < 4; i++) { cooldown_until.push(readU64LE(dv, o)); o += 8; }
      for (let i = 0; i < 4; i++) { last_trigger_tick.push(readU64LE(dv, o)); o += 8; }
      const recent_trigger_count = Array.from(bytes.slice(o, o + 4)); o += 4;
      for (let i = 0; i < 4; i++) { recovery_tick.push(readU64LE(dv, o)); o += 8; }
      const cb1_collateral_index = bytes[o]; o += 1;
      const mint_rate_limit = readU64LE(dv, o); o += 8;
      const optimizer_enabled = !!bytes[o]; o += 1;
      const learning_rate_scale = readU64LE(dv, o); o += 8;
      const max_activation_duration = readU64LE(dv, o); o += 8;
      const bump = bytes[o];

      return {
        status, activation_tick, trigger_count, cooldown_until, last_trigger_tick,
        recent_trigger_count, recovery_tick, cb1_collateral_index, mint_rate_limit,
        optimizer_enabled, learning_rate_scale, max_activation_duration, bump
      };
    }

    function parseAgentRecord(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 116) throw new Error("AgentRecord too short");
      let o = 8;
      const agent = readPubkey(bytes, o); o += 32;
      const stake = readU64LE(dv, o); o += 8;
      const reputation = readU64LE(dv, o); o += 8;
      const role = bytes[o]; o += 1;
      const tier = bytes[o]; o += 1;
      const status = bytes[o]; o += 1;
      const proposals_submitted = readU64LE(dv, o); o += 8;
      const proposals_accepted = readU64LE(dv, o); o += 8;
      const registered_at = readI64LE(dv, o); o += 8;
      const registered_slot = readU64LE(dv, o); o += 8;
      const last_active_at = readI64LE(dv, o); o += 8;
      const agent_score = readU64LE(dv, o); o += 8;
      const last_slashed_slot = readU64LE(dv, o); o += 8;
      const bump = bytes[o];

      return {
        agent, stake, reputation, role, tier, status, proposals_submitted,
        proposals_accepted, registered_at, registered_slot, last_active_at,
        agent_score, last_slashed_slot, bump
      };
    }

    function parseCollateralVault(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 202) throw new Error("CollateralVault too short");
      let o = 8;
      const index = bytes[o]; o += 1;
      const mint = readPubkey(bytes, o); o += 32;
      const vault = readPubkey(bytes, o); o += 32;
      const oracle = readPubkey(bytes, o); o += 32;
      const risk_score = readU64LE(dv, o); o += 8;
      const weight_cap = readU64LE(dv, o); o += 8;
      const base_weight_cap = readU64LE(dv, o); o += 8;
      const price = readU64LE(dv, o); o += 8;
      const confidence = readU64LE(dv, o); o += 8;
      const last_oracle_slot = readU64LE(dv, o); o += 8;
      const total_deposits = readU64LE(dv, o); o += 8;
      const bump = bytes[o]; o += 1;
      const pyth_price_feed = readPubkey(bytes, o); o += 32;
      let twap_price = null;
      if (bytes.length >= o + 8) {
        twap_price = readU64LE(dv, o); o += 8;
      }
      return {
        index, mint, vault, oracle, risk_score, weight_cap, base_weight_cap,
        price, confidence, last_oracle_slot, total_deposits, bump, pyth_price_feed, twap_price
      };
    }

    function pow10n(exp) {
      let out = 1n;
      for (let i = 0; i < exp; i++) out *= 10n;
      return out;
    }

    function scaleToSixUnsigned(v, exponent) {
      const shift = exponent + 6;
      if (shift >= 0) return v * pow10n(shift);
      const factor = pow10n(-shift);
      return (v + factor - 1n) / factor;
    }

    function parsePythPriceUpdate(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 8 + 32 + 1 + 32 + 8 + 8 + 4 + 8 + 8 + 8 + 8 + 8) {
        throw new Error("Pyth account too short");
      }

      let o = 8;
      const write_authority = readPubkey(bytes, o); o += 32;
      const verTag = bytes[o]; o += 1;
      let verification = "Unknown";
      if (verTag === 0) {
        verification = `Partial(${bytes[o]})`;
        o += 1;
      } else if (verTag === 1) {
        verification = "Full";
      }

      const feed_id = bytes.slice(o, o + 32); o += 32;
      const priceRaw = readI64LE(dv, o); o += 8;
      const confRaw = readU64LE(dv, o); o += 8;
      const exponent = readI32LE(dv, o); o += 4;
      const publish_time = Number(readI64LE(dv, o)); o += 8;
      const prev_publish_time = Number(readI64LE(dv, o)); o += 8;
      const ema_price = readI64LE(dv, o); o += 8;
      const ema_conf = readU64LE(dv, o); o += 8;
      const posted_slot = readU64LE(dv, o); o += 8;

      if (priceRaw <= 0n) throw new Error("Non-positive price");
      const priceScaled = scaleToSixUnsigned(priceRaw, exponent);
      const confScaled = scaleToSixUnsigned(confRaw, exponent);

      return {
        write_authority,
        verification,
        feed_id_hex: Array.from(feed_id).map((x) => x.toString(16).padStart(2, "0")).join(""),
        price: Number(priceScaled) / 1e6,
        conf: Number(confScaled) / 1e6,
        exponent,
        publish_time,
        prev_publish_time,
        ema_price,
        ema_conf,
        posted_slot: Number(posted_slot)
      };
    }

    function classifyTx(logs = []) {
      const raw = logs.join(" ").toLowerCase();
      const n = raw.replace(/[^a-z]/g, "");
      if (n.includes("instructionrebalance") || n.includes("instructioncommitrebalance")) return "rebalance";
      if (n.includes("instructionupdateoracle") || n.includes("instructionupdateoraclepyth") || n.includes("instructionsetpythfeed")) return "oracle";
      if (n.includes("instructionmint")) return "mint";
      if (n.includes("instructionredeem")) return "redeem";
      if (
        n.includes("instructionregisteragent") ||
        n.includes("instructionupdateagentscore") ||
        n.includes("instructionpromoteagent") ||
        n.includes("instructiondemoteagent") ||
        n.includes("instructionslashagent")
      ) return "agent";
      return "oracle";
    }

    function tierLabel(tier) {
      if (tier >= 3) return "⭐⭐⭐ (Tier 3)";
      if (tier === 2) return "⭐⭐ (Tier 2)";
      if (tier === 1) return "⭐ (Tier 1)";
      return "Tier 0";
    }

    function statusLabel(s) {
      return AGENT_STATUS[s] || `Unknown(${s})`;
    }

    function roleLabel(r) {
      return ROLE_MAP[r] || `Role ${r}`;
    }

    function setLive(ok) {
      $("liveText").textContent = ok ? "LIVE" : "OFFLINE";
      $("liveDot").classList.toggle("live", ok);
      $("rpcHealth").textContent = ok ? "RPC: OK" : "RPC: DEGRADED";
      $("rpcHealth").className = `badge ${ok ? "ok" : "bad"}`;
    }

    function updateHeaderTick() {
      $("lastUpdateAge").textContent = fmtAgoMs(state.lastSuccessAt);
      $("keeperActivity").textContent = fmtAgoUnix(state.lastKeeperActivity);
    }

    function computeRisk(crPct, protocol, circuit) {
      const activeCount = (circuit?.status || []).filter((s) => s > 0).length;
      if (protocol?.emergency_shutdown) return ["CRITICAL", "risk-critical"];
      if (crPct < 120 || activeCount >= 2) return ["CRITICAL", "risk-critical"];
      if (crPct < 130 || activeCount === 1) return ["HIGH", "risk-high"];
      if (crPct < 150) return ["ELEVATED", "risk-elevated"];
      return ["NORMAL", "risk-normal"];
    }

    function ensureWeightRows() {
      if (state.weightRowsReady) return;
      const labels = ["USDC", "USDT", "DAI", "RESERVE"];
      const box = $("weightsBox");
      box.replaceChildren();
      labels.forEach((label, i) => {
        const row = document.createElement("div");
        row.className = "weight-row";
        row.id = `weight-${i}`;

        const head = document.createElement("div");
        head.className = "weight-head";
        const left = document.createElement("span");
        left.textContent = label;
        const right = document.createElement("span");
        right.className = "weight-pct";
        right.textContent = "--";
        head.append(left, right);

        const bar = document.createElement("div");
        bar.className = "bar";
        const fill = document.createElement("div");
        fill.className = "bar-fill weight-fill";
        fill.style.width = "0%";
        bar.appendChild(fill);

        row.append(head, bar);
        box.appendChild(row);
      });
      state.weightRowsReady = true;
    }

    function updateHealth(protocol, circuit, supplyRawBigInt, vaults) {
      if (!protocol) return;

      const supply = supplyRawBigInt > 0n ? supplyRawBigInt : protocol.total_supply;
      let collateralValue = 0n;
      for (const v of vaults) {
        collateralValue += (v.total_deposits * v.price) / 1000000n;
      }

      const hasSupply = supply > 0n;
      const crPct = hasSupply ? Number((collateralValue * 10000n) / supply) / 100 : null;
      if (crPct !== null) {
        animateNumber($("crValue"), crPct, { decimals: 2, suffix: "%" });
      } else {
        $("crValue").textContent = "N/A";
      }

      const fillPct = crPct !== null ? Math.max(0, Math.min(100, (crPct / 200) * 100)) : 0;
      const bar = $("crBar");
      bar.style.width = `${fillPct.toFixed(1)}%`;
      bar.style.background = !hasSupply ? "var(--muted)" : crPct > 150 ? "var(--green)" : crPct >= 120 ? "var(--amber)" : "var(--red)";

      const [riskText, riskClass] = computeRisk(crPct ?? 999, protocol, circuit);
      const rb = $("riskBadge");
      rb.className = `risk-badge ${riskClass}`;
      rb.textContent = `RISK: ${riskText}`;

      $("mstbSupply").textContent = fmtToken(supplyRawBigInt, 6, 2);
      $("stateSupply").textContent = fmtToken(protocol.total_supply, 6, 2);
      $("emergencyFlag").textContent = protocol.emergency_shutdown ? "YES" : "NO";
      $("emergencyFlag").className = protocol.emergency_shutdown ? "bad" : "ok";

      const activeCount = (circuit?.status || []).filter((s) => s > 0).length;
      $("breakerFlag").textContent = `${activeCount} active`;
      $("breakerFlag").className = activeCount > 0 ? "warn" : "ok";

      ensureWeightRows();
      const nextWeights = (protocol.weights || []).slice(0, 4).map((w) => ppmToPct(w || 0n));
      for (let i = 0; i < 4; i++) {
        const row = $(`weight-${i}`);
        if (!row) continue;
        const pct = Number(nextWeights[i] ?? 0);
        const prevPct = Number(state.prevWeights[i] ?? NaN);
        if (!Number.isFinite(prevPct) || prevPct !== pct) {
          const pctEl = row.querySelector(".weight-pct");
          const fillEl = row.querySelector(".bar-fill");
          const width = `${Math.max(0, Math.min(100, pct)).toFixed(2)}%`;
          if (fillEl.style.width !== width) fillEl.style.width = width;
          setIfChanged(pctEl, `${pct.toFixed(2)}%`);
        }
      }
      state.prevWeights = nextWeights;
    }

    function updateOptimizer(protocol) {
      if (!protocol) return;
      const cr = ppmToPct(protocol.cr_target);
      const mintFee = ppmToPct(protocol.mint_fee_rate);
      const redeemFee = ppmToPct(protocol.redeem_fee_rate);

      $("paramCr").textContent = `${cr.toFixed(2)}%`;
      $("paramMintFee").textContent = `${mintFee.toFixed(4)}%`;
      $("paramRedeemFee").textContent = `${redeemFee.toFixed(4)}%`;
      $("paramWeights").textContent = protocol.weights.map((w) => `${ppmToPct(w).toFixed(1)}%`).join(" / ");

      const snapshot = {
        t: Date.now(),
        cr_target: Number(protocol.cr_target),
        mint_fee_rate: Number(protocol.mint_fee_rate),
        redeem_fee_rate: Number(protocol.redeem_fee_rate),
        weights: protocol.weights.map((x) => Number(x))
      };

      const last = state.history[state.history.length - 1];
      const changed = !last ||
        last.cr_target !== snapshot.cr_target ||
        last.mint_fee_rate !== snapshot.mint_fee_rate ||
        last.redeem_fee_rate !== snapshot.redeem_fee_rate ||
        JSON.stringify(last.weights) !== JSON.stringify(snapshot.weights);

      if (changed) {
        state.history.push(snapshot);
        if (state.history.length > 200) state.history = state.history.slice(-200);
        saveJson(CFG.HISTORY_KEY, state.history);
      }

      let optimizerStatus = "Converging";
      if (state.history.length >= 2) {
        const a = state.history[state.history.length - 1];
        const b = state.history[state.history.length - 2];
        const delta =
          Math.abs(a.cr_target - b.cr_target) +
          Math.abs(a.mint_fee_rate - b.mint_fee_rate) +
          Math.abs(a.redeem_fee_rate - b.redeem_fee_rate) +
          a.weights.reduce((acc, v, i) => acc + Math.abs(v - (b.weights?.[i] || 0)), 0);
        optimizerStatus = delta < 2500 ? "Stable" : "Converging";
      }
      $("optimizerStatus").textContent = optimizerStatus;
      $("optimizerStatus").className = optimizerStatus === "Stable" ? "ok" : "warn";

      drawHistoryChart();
    }

    function drawHistoryChart() {
      const canvas = $("historyChart");
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      const w = Math.max(300, Math.floor(rect.width));
      const h = Math.max(150, Math.floor(rect.height));
      canvas.width = Math.floor(w * dpr);
      canvas.height = Math.floor(h * dpr);

      const ctx = canvas.getContext("2d");
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);

      ctx.strokeStyle = "rgba(255,255,255,0.08)";
      for (let i = 0; i <= 4; i++) {
        const y = (h - 20) * (i / 4) + 10;
        ctx.beginPath();
        ctx.moveTo(8, y);
        ctx.lineTo(w - 8, y);
        ctx.stroke();
      }

      const data = state.history.slice(-100);
      if (data.length < 2) {
        ctx.fillStyle = "#7f95ad";
        ctx.font = "12px monospace";
        ctx.fillText("Waiting for parameter history...", 12, h / 2);
        return;
      }

      const series = {
        cr: data.map((d) => d.cr_target),
        mf: data.map((d) => d.mint_fee_rate),
        rf: data.map((d) => d.redeem_fee_rate),
        w0: data.map((d) => d.weights[0] || 0),
        w1: data.map((d) => d.weights[1] || 0),
        w2: data.map((d) => d.weights[2] || 0),
        w3: data.map((d) => d.weights[3] || 0)
      };

      const colors = {
        cr: "#00f0ff",
        mf: "#00ff88",
        rf: "#ffaa00",
        w0: "#ff3366",
        w1: "#9b6bff",
        w2: "#36d0ff",
        w3: "#7f95ad"
      };

      function drawLine(values, color) {
        const min = Math.min(...values);
        const max = Math.max(...values);
        const span = Math.max(1, max - min);
        ctx.strokeStyle = color;
        ctx.lineWidth = 1.7;
        ctx.beginPath();
        values.forEach((v, i) => {
          const x = 10 + (i / (values.length - 1)) * (w - 20);
          const n = (v - min) / span;
          const y = 10 + (1 - n) * (h - 20);
          if (i === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        });
        ctx.stroke();
      }

      Object.keys(series).forEach((key) => drawLine(series[key], colors[key]));
    }

    function createPlaceholderRow(colspan, text) {
      const tr = document.createElement("tr");
      tr.dataset.placeholder = "1";
      const td = document.createElement("td");
      td.colSpan = colspan;
      td.className = "muted";
      td.textContent = text;
      tr.appendChild(td);
      return tr;
    }

    function clearPlaceholderRows(body) {
      Array.from(body.querySelectorAll("tr")).forEach((row) => {
        if (row.dataset.placeholder === "1") {
          row.remove();
          return;
        }
        const onlyCell = row.children.length === 1 ? row.children[0] : null;
        if (onlyCell && Number(onlyCell.colSpan) > 1) row.remove();
      });
    }

    function renderAgentIdentityCell(cell, name, agent) {
      let nameEl = cell.querySelector("strong");
      if (!nameEl) {
        nameEl = document.createElement("strong");
        cell.appendChild(nameEl);
      }
      if (nameEl.textContent !== name) nameEl.textContent = name;

      let br = cell.querySelector("br");
      if (!br) {
        br = document.createElement("br");
        cell.appendChild(br);
      }

      let keyEl = cell.querySelector("span.muted");
      if (!keyEl) {
        keyEl = document.createElement("span");
        keyEl.className = "muted";
        cell.appendChild(keyEl);
      }
      const next = shortKey(agent);
      if (keyEl.textContent !== next) keyEl.textContent = next;
    }

    function createAgentRow(row) {
      const tr = document.createElement("tr");
      tr.dataset.agent = row.agent;

      const rankCell = document.createElement("td");
      rankCell.textContent = "#1";

      const identityCell = document.createElement("td");
      renderAgentIdentityCell(identityCell, row.name, row.agent);

      const tierCell = document.createElement("td");
      tierCell.className = "tier";
      tierCell.textContent = tierLabel(row.tier);

      const roleCell = document.createElement("td");
      roleCell.textContent = row.role;

      const scoreCell = document.createElement("td");
      const scoreValue = document.createElement("span");
      scoreValue.className = "score-cell";
      scoreValue.textContent = row.score.toLocaleString();
      scoreCell.appendChild(scoreValue);

      const statusCell = document.createElement("td");
      statusCell.textContent = row.status;

      tr.append(rankCell, identityCell, tierCell, roleCell, scoreCell, statusCell);
      return tr;
    }

    function updateAgents(records, protocol) {
      const body = $("agentRows");
      if (!records.length) {
        for (const row of state.agentRows.values()) row.remove();
        state.agentRows.clear();
        state.prevAgents = {};
        clearPlaceholderRows(body);
        if (body.children.length === 0) {
          body.appendChild(createPlaceholderRow(6, "No agent records found on-chain."));
        }
        return;
      }

      clearPlaceholderRows(body);

      const keeperMap = new Map();
      (protocol?.keeper_set || []).forEach((k, i) => keeperMap.set(k, `keeper${i + 1}`));

      const rows = records.map((r) => ({
        agent: r.agent,
        name: keeperMap.get(r.agent) || `${roleLabel(r.role)}-${shortKey(r.agent)}`,
        tier: r.tier,
        role: roleLabel(r.role),
        status: statusLabel(r.status),
        score: Number(r.agent_score || r.reputation || 0n)
      })).sort((a, b) => b.score - a.score);

      const nextPrevAgents = {};
      const activeAgents = new Set(rows.map((r) => r.agent));

      for (let idx = 0; idx < rows.length; idx++) {
        const r = rows[idx];
        let tr = state.agentRows.get(r.agent);
        if (!tr) {
          tr = createAgentRow(r);
          state.agentRows.set(r.agent, tr);
          body.appendChild(tr);
        }

        if (body.children[idx] !== tr) {
          body.insertBefore(tr, body.children[idx] || null);
        }

        const prevAgent = state.prevAgents[r.agent];
        const cells = tr.children;

        setTextOnlyIfChanged(cells[0], `#${idx + 1}`);
        renderAgentIdentityCell(cells[1], r.name, r.agent);
        setTextOnlyIfChanged(cells[2], tierLabel(r.tier));
        setTextOnlyIfChanged(cells[3], r.role);

        const scoreEl = cells[4].querySelector(".score-cell");
        const nextScoreTxt = r.score.toLocaleString();
        if (!prevAgent) {
          scoreEl.textContent = nextScoreTxt;
        } else if (prevAgent.score !== r.score) {
          scoreEl.textContent = nextScoreTxt;
          scoreEl.classList.remove("score-up", "score-down");
          if (r.score > prevAgent.score) scoreEl.classList.add("score-up");
          if (r.score < prevAgent.score) scoreEl.classList.add("score-down");
          setTimeout(() => scoreEl.classList.remove("score-up", "score-down"), 700);
        } else if (scoreEl.textContent !== nextScoreTxt) {
          scoreEl.textContent = nextScoreTxt;
        }

        setTextOnlyIfChanged(cells[5], r.status);

        nextPrevAgents[r.agent] = {
          score: r.score,
          tier: r.tier,
          role: r.role,
          status: r.status,
          name: r.name
        };
      }

      for (const [agent, rowEl] of state.agentRows.entries()) {
        if (activeAgents.has(agent)) continue;
        rowEl.remove();
        state.agentRows.delete(agent);
      }

      state.prevAgents = nextPrevAgents;
    }

    async function enrichTxTypes(sigs) {
      const jobs = sigs.map(async (s) => {
        try {
          const tx = await rpc("getTransaction", [s.signature, { encoding: "json", commitment: "confirmed", maxSupportedTransactionVersion: 0 }], 10000);
          const logs = tx?.meta?.logMessages || [];
          return { sig: s.signature, type: classifyTx(logs) };
        } catch {
          return { sig: s.signature, type: "oracle" };
        }
      });
      const out = await Promise.all(jobs);
      const map = new Map();
      out.forEach((x) => map.set(x.sig, x.type));
      return map;
    }

    function createTxRow(tx) {
      const tr = document.createElement("tr");
      tr.dataset.sig = tx.sig;

      const timeCell = document.createElement("td");
      timeCell.textContent = tx.blockTime ? fmtClock(tx.blockTime) : "--";

      const typeCell = document.createElement("td");
      typeCell.textContent = tx.type;

      const sigCell = document.createElement("td");
      const link = document.createElement("a");
      link.className = "tx-link";
      link.target = "_blank";
      link.rel = "noreferrer";
      link.href = `https://explorer.solana.com/tx/${tx.sig}?cluster=devnet`;
      link.textContent = shortKey(tx.sig);
      sigCell.appendChild(link);

      const statusCell = document.createElement("td");
      statusCell.textContent = tx.status;

      tr.append(timeCell, typeCell, sigCell, statusCell);
      return tr;
    }

    function updateTxFeed(signatures, typeMap) {
      const body = $("txRows");
      const top = (signatures || []).slice(0, 10);
      if (!top.length) {
        for (const row of state.txRows.values()) row.remove();
        state.txRows.clear();
        state.prevTxSigs = new Set();
        clearPlaceholderRows(body);
        if (body.children.length === 0) {
          body.appendChild(createPlaceholderRow(4, "No recent transactions."));
        }
        return;
      }

      clearPlaceholderRows(body);

      const nextRows = top.map((s) => {
        const sig = s.signature;
        return {
          sig,
          blockTime: s.blockTime || 0,
          status: s.err ? "❌" : "✅",
          type: typeMap.get(sig) || "oracle"
        };
      });

      const nextSigSet = new Set(nextRows.map((r) => r.sig));

      for (const [sig, rowEl] of state.txRows.entries()) {
        if (nextSigSet.has(sig)) continue;
        rowEl.remove();
        state.txRows.delete(sig);
      }

      for (let idx = 0; idx < nextRows.length; idx++) {
        const rowData = nextRows[idx];
        let tr = state.txRows.get(rowData.sig);
        const isNew = !state.prevTxSigs.has(rowData.sig) && state.prevTxSigs.size > 0;

        if (!tr) {
          tr = createTxRow(rowData);
          if (isNew) tr.classList.add("tx-new");
          state.txRows.set(rowData.sig, tr);
          body.appendChild(tr);
        }

        const cells = tr.children;
        setIfChanged(cells[0], rowData.blockTime ? fmtClock(rowData.blockTime) : "--");
        setIfChanged(cells[1], rowData.type);
        const link = cells[2].querySelector("a");
        const href = `https://explorer.solana.com/tx/${rowData.sig}?cluster=devnet`;
        if (link.getAttribute("href") !== href) link.setAttribute("href", href);
        setIfChanged(link, shortKey(rowData.sig));
        setIfChanged(cells[3], rowData.status);

        if (body.children[idx] !== tr) {
          body.insertBefore(tr, body.children[idx] || null);
        }
      }

      while (body.children.length > 10) {
        const last = body.lastElementChild;
        const sig = last?.dataset?.sig;
        last?.remove();
        if (sig) state.txRows.delete(sig);
      }

      state.prevTxSigs = nextSigSet;
    }

    function ensureOracleCards() {
      if (state.oracleCardsReady) return;
      const strip = $("oracleStrip");
      strip.replaceChildren();
      ["USDC", "USDT", "DAI"].forEach((sym) => {
        const card = document.createElement("article");
        card.className = "oracle-card";
        card.id = `oracle-${sym.toLowerCase()}`;

        const symEl = document.createElement("div");
        symEl.className = "sym";
        symEl.textContent = `${sym}/USD`;

        const priceEl = document.createElement("div");
        priceEl.className = "price";
        priceEl.textContent = "--";

        const metaEl = document.createElement("div");
        metaEl.className = "meta";
        metaEl.appendChild(document.createTextNode("Confidence: "));
        const confEl = document.createElement("span");
        confEl.className = "oracle-conf muted";
        confEl.textContent = "--";
        metaEl.appendChild(confEl);
        metaEl.appendChild(document.createElement("br"));
        metaEl.appendChild(document.createTextNode("Updated: "));
        const updatedEl = document.createElement("span");
        updatedEl.className = "oracle-updated muted";
        updatedEl.textContent = "--";
        metaEl.appendChild(updatedEl);

        card.append(symEl, priceEl, metaEl);
        strip.appendChild(card);
      });
      state.oracleCardsReady = true;
    }

    function updateOracles(oracleData) {
      ensureOracleCards();
      ["USDC", "USDT", "DAI"].forEach((sym) => {
        const card = $(`oracle-${sym.toLowerCase()}`);
        if (!card) return;

        const priceEl = card.querySelector(".price");
        const confEl = card.querySelector(".oracle-conf");
        const updatedEl = card.querySelector(".oracle-updated");
        const value = oracleData?.[sym];

        if (!value || value.error) {
          const errText = value?.error || "RPC timeout";
          setIfChanged(priceEl, "--");
          setIfChanged(confEl, "Price unavailable");
          setIfChanged(updatedEl, errText);
          priceEl.classList.add("bad");
          confEl.className = "oracle-conf bad";
          updatedEl.className = "oracle-updated muted";

          state.prevOracles[sym] = {
            price: "--",
            conf: "Price unavailable",
            updated: errText,
            confClass: "bad"
          };
          return;
        }

        const confClass = value.conf <= 0.005 ? "ok" : value.conf <= 0.02 ? "warn" : "bad";
        const priceTxt = `$${value.price.toFixed(6)}`;
        const confTxt = `±$${value.conf.toFixed(6)}`;
        const updatedTxt = `${fmtClock(value.publish_time)} (${fmtAgoUnix(value.publish_time)})`;

        setIfChanged(priceEl, priceTxt);
        setIfChanged(confEl, confTxt);
        setIfChanged(updatedEl, updatedTxt);

        priceEl.classList.remove("bad");
        confEl.className = `oracle-conf ${confClass}`;
        updatedEl.className = "oracle-updated muted";

        state.prevOracles[sym] = {
          price: priceTxt,
          conf: confTxt,
          updated: updatedTxt,
          confClass
        };
      });
    }

    function parseUiAmountToRaw(value, decimals = 6) {
      const text = String(value ?? "").trim();
      if (!text) return 0n;
      if (!/^\d+(\.\d+)?$/.test(text)) return null;
      const [ints, frac = ""] = text.split(".");
      const fracPadded = (frac + "0".repeat(decimals)).slice(0, decimals);
      return BigInt(ints || "0") * pow10n(decimals) + BigInt(fracPadded || "0");
    }

    function uiNumberToTrimmedText(n, maxFrac = 6) {
      const fixed = Number(n || 0).toFixed(maxFrac);
      return fixed.replace(/\.0+$/, "").replace(/(\.\d*?)0+$/, "$1");
    }

    function uiAmountToRawFromNumber(value, decimals = 6) {
      const txt = uiNumberToTrimmedText(value, decimals);
      return parseUiAmountToRaw(txt, decimals) || 0n;
    }

    function walletBalanceRaw(symbol, decimals = 6) {
      return uiAmountToRawFromNumber(state.wallet.balances?.[symbol] || 0, decimals);
    }

    function resolveCollateralMints(protocol, vaults) {
      const next = {
        0: protocol?.usdc_mint || null,
        1: protocol?.usdt_mint || null,
        2: protocol?.dai_mint || null,
        3: protocol?.usds_mint || null
      };
      for (const v of vaults || []) {
        if (next[v.index]) continue;
        if (v.index >= 0 && v.index <= 3) next[v.index] = v.mint;
      }
      return next;
    }

    function initSolanaContext() {
      if (!window.solanaWeb3) throw new Error("solanaWeb3 CDN failed to load");
      const w3 = window.solanaWeb3;
      if (!state.connection || state.connection._rpcEndpoint !== CFG.RPC_URL) {
        state.connection = new w3.Connection(CFG.RPC_URL, "confirmed");
      }
      if (!state.pubkeys) {
        const te = new TextEncoder();
        const programId = new w3.PublicKey(CFG.PROGRAM_ID);
        const protocolState = new w3.PublicKey(CFG.PROTOCOL_STATE);
        const circuitBreaker = new w3.PublicKey(CFG.CIRCUIT_BREAKER);
        const mstbMint = new w3.PublicKey(CFG.MSTB_MINT);
        const tokenProgram = new w3.PublicKey(CFG.TOKEN_PROGRAM);
        const associatedTokenProgram = new w3.PublicKey(CFG.ASSOCIATED_TOKEN_PROGRAM);

        const [derivedProtocolState] = w3.PublicKey.findProgramAddressSync([te.encode("protocol_state")], programId);
        const [derivedCircuitBreaker] = w3.PublicKey.findProgramAddressSync([te.encode("circuit_breaker")], programId);

        const vaultPdas = [0, 1, 2, 3].map((i) =>
          w3.PublicKey.findProgramAddressSync([te.encode("collateral_vault"), Uint8Array.from([i])], programId)[0]
        );

        if (!derivedProtocolState.equals(protocolState)) {
          console.warn("Protocol state PDA mismatch", derivedProtocolState.toBase58(), protocolState.toBase58());
        }
        if (!derivedCircuitBreaker.equals(circuitBreaker)) {
          console.warn("Circuit breaker PDA mismatch", derivedCircuitBreaker.toBase58(), circuitBreaker.toBase58());
        }

        state.pubkeys = {
          programId,
          protocolState,
          circuitBreaker,
          mstbMint,
          tokenProgram,
          associatedTokenProgram,
          systemProgram: w3.SystemProgram.programId,
          rent: w3.SYSVAR_RENT_PUBKEY,
          vaultPdas
        };
      }
    }

    function deriveAtaAddress(owner, mint) {
      const w3 = window.solanaWeb3;
      return w3.PublicKey.findProgramAddressSync(
        [owner.toBytes(), state.pubkeys.tokenProgram.toBytes(), mint.toBytes()],
        state.pubkeys.associatedTokenProgram
      )[0];
    }

    function createAtaIdempotentIx(payer, ata, owner, mint) {
      const w3 = window.solanaWeb3;
      return new w3.TransactionInstruction({
        programId: state.pubkeys.associatedTokenProgram,
        keys: [
          { pubkey: payer, isSigner: true, isWritable: true },
          { pubkey: ata, isSigner: false, isWritable: true },
          { pubkey: owner, isSigner: false, isWritable: false },
          { pubkey: mint, isSigner: false, isWritable: false },
          { pubkey: state.pubkeys.systemProgram, isSigner: false, isWritable: false },
          { pubkey: state.pubkeys.tokenProgram, isSigner: false, isWritable: false }
        ],
        data: new Uint8Array([1])
      });
    }

    async function getInstructionDiscriminator(name, fallback) {
      if (state.instructionDiscriminators[name]) {
        return state.instructionDiscriminators[name];
      }
      const fallbackBytes = new Uint8Array(fallback);
      if (!window.crypto?.subtle) {
        state.instructionDiscriminators[name] = fallbackBytes;
        return fallbackBytes;
      }
      const bytes = new TextEncoder().encode(`global:${name}`);
      const hash = await window.crypto.subtle.digest("SHA-256", bytes);
      const discriminator = new Uint8Array(hash).slice(0, 8);
      state.instructionDiscriminators[name] = discriminator;
      return discriminator;
    }

    function writeU64LE(out, offset, value) {
      let x = BigInt(value);
      for (let i = 0; i < 8; i++) {
        out[offset + i] = Number(x & 0xffn);
        x >>= 8n;
      }
    }

    async function getMintDiscriminator() {
      return getInstructionDiscriminator("mint", [51, 57, 225, 47, 182, 146, 137, 166]);
    }

    async function getRedeemDiscriminator() {
      return getInstructionDiscriminator("redeem", [184, 12, 86, 149, 70, 196, 97, 225]);
    }

    async function getRegisterAgentDiscriminator() {
      return getInstructionDiscriminator("register_agent", [135, 157, 66, 195, 2, 113, 175, 30]);
    }

    function encodeMintInstructionData(collateralIndex, collateralAmountRaw, maxPriceRaw, discriminator) {
      const out = new Uint8Array(25);
      out.set(discriminator, 0);
      out[8] = collateralIndex & 0xff;
      writeU64LE(out, 9, collateralAmountRaw);
      writeU64LE(out, 17, maxPriceRaw);
      return out;
    }

    function encodeRedeemInstructionData(musdAmountRaw, minOutAmountRaw, discriminator) {
      const out = new Uint8Array(24);
      out.set(discriminator, 0);
      writeU64LE(out, 8, musdAmountRaw);
      writeU64LE(out, 16, minOutAmountRaw);
      return out;
    }

    function encodeRegisterAgentInstructionData(roleIndex, stakeLamportsRaw, discriminator) {
      const out = new Uint8Array(17);
      out.set(discriminator, 0);
      out[8] = roleIndex & 0xff;
      writeU64LE(out, 9, stakeLamportsRaw);
      return out;
    }

    function walletProvider() {
      const provider = window.solana;
      if (provider?.isPhantom) return provider;
      return null;
    }

    function renderWalletControls() {
      const connectBtn = $("walletConnectBtn");
      const connectLabel = $("walletConnectLabel");
      const disconnectBtn = $("walletDisconnectBtn");
      const installLink = $("walletInstallLink");

      const installed = !!state.wallet.provider;
      const connected = !!state.wallet.publicKey;

      connectBtn.hidden = !installed;
      installLink.hidden = installed;

      if (!installed) {
        disconnectBtn.hidden = true;
        connectLabel.textContent = "Connect Wallet";
        connectBtn.disabled = true;
        return;
      }

      if (!connected) {
        connectLabel.textContent = "Connect Wallet";
        connectBtn.disabled = false;
        disconnectBtn.hidden = true;
      } else {
        connectLabel.textContent = shortKey(state.wallet.publicKey.toBase58());
        connectBtn.disabled = true;
        disconnectBtn.hidden = false;
      }
    }

    function renderWalletBalances() {
      const connected = !!state.wallet.publicKey;
      const b = state.wallet.balances;

      $("walletAddressView").textContent = connected ? state.wallet.publicKey.toBase58() : "--";
      const solEl = $("balSOL");
      if (solEl) solEl.textContent = connected ? (b.SOL ?? 0).toFixed(4) : "--";
      $("balUSDC").textContent = connected ? uiNumberToTrimmedText(b.USDC) : "--";
      $("balUSDT").textContent = connected ? uiNumberToTrimmedText(b.USDT) : "--";
      $("balDAI").textContent = connected ? uiNumberToTrimmedText(b.DAI) : "--";
      $("balMSTB").textContent = connected ? uiNumberToTrimmedText(b.MSTB) : "--";

      $("faucetBalUSDC").textContent = connected ? uiNumberToTrimmedText(b.USDC) : "--";
      $("faucetBalUSDT").textContent = connected ? uiNumberToTrimmedText(b.USDT) : "--";
      $("faucetBalDAI").textContent = connected ? uiNumberToTrimmedText(b.DAI) : "--";
      $("faucetBalMSTB").textContent = connected ? uiNumberToTrimmedText(b.MSTB) : "--";
    }

    function parseSplMintMeta(bytes) {
      const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
      if (bytes.length < 82) return { mintAuthority: null, decimals: null };
      const mintAuthorityOption = readU32LE(dv, 0);
      const mintAuthority = mintAuthorityOption === 1 ? readPubkey(bytes, 4) : null;
      const decimals = bytes[44];
      return { mintAuthority, decimals };
    }

    function setFaucetButtonsDisabled(disabled, tooltip) {
      ["faucetSolBtn", "faucetUsdcBtn", "faucetUsdtBtn", "faucetDaiBtn"].forEach((id) => {
        const btn = $(id);
        if (!btn) return;
        btn.disabled = !!disabled;
        if (tooltip) btn.title = tooltip;
      });
    }

    function renderFaucetStatus() {
      const statusEl = $("faucetStatus");
      const connected = !!state.wallet.publicKey;
      const mintsReady = !!state.collateralMints[0] && !!state.collateralMints[1] && !!state.collateralMints[2];
      if (!mintsReady) {
        statusEl.className = "faucet-status muted";
        statusEl.textContent = "Waiting for collateral mint metadata...";
        setFaucetButtonsDisabled(true, FAUCET_CONFIG.hint);
        return;
      }

      if (state.faucet.airdropBusy) {
        statusEl.className = "faucet-status warn";
        statusEl.textContent = "Processing faucet request...";
        setFaucetButtonsDisabled(true, "Request in progress");
        return;
      }

      if (FAUCET_CONFIG.instructionAvailable) {
        statusEl.className = "faucet-status ok";
        statusEl.textContent = connected ? "🚰 Devnet faucet ready — get SOL for gas + test tokens to mint MSTB." : "Connect wallet to use devnet faucet.";
        setFaucetButtonsDisabled(!connected, connected ? "Click to request tokens" : "Connect wallet first");
        $("faucetUsdcBtn").textContent = "Get 1,000 USDC";
        $("faucetUsdtBtn").textContent = "Get 1,000 USDT";
        $("faucetDaiBtn").textContent = "Get 10 DAI";
        return;
      }

      if (!connected) {
        statusEl.className = "faucet-status muted";
        statusEl.textContent = "Connect wallet to use devnet faucet.";
        setFaucetButtonsDisabled(true, "Connect wallet first");
        return;
      }

      statusEl.className = "faucet-status warn";
      statusEl.textContent = "Token faucet unavailable. SOL airdrop only.";
      setFaucetButtonsDisabled(false, "Request 1 SOL devnet airdrop for fees");
      ["faucetUsdcBtn", "faucetUsdtBtn", "faucetDaiBtn"].forEach((id) => {
        $(id).textContent = "Get 1 SOL (gas)";
      });
    }

    async function refreshFaucetMintAuthorities(opts = {}) {
      const silent = !!opts.silent;
      const idxs = [0, 1, 2].filter((i) => !!state.collateralMints[i]);
      if (idxs.length < 3 || state.faucet.checking) {
        renderFaucetStatus();
        return;
      }

      const mintsKey = idxs.map((i) => state.collateralMints[i]).join("|");
      if (state.faucet.checked && state.faucet.lastMintsKey === mintsKey) {
        renderFaucetStatus();
        return;
      }

      state.faucet.checking = true;
      try {
        const mintPubkeys = idxs.map((i) => state.collateralMints[i]);
        const result = await rpc("getMultipleAccounts", [mintPubkeys, { encoding: "base64" }], 12000);
        const nextAuthorities = {};
        idxs.forEach((idx, pos) => {
          const b64 = result?.value?.[pos]?.data?.[0];
          if (!b64) return;
          try {
            const bytes = base64ToBytes(b64);
            nextAuthorities[idx] = parseSplMintMeta(bytes).mintAuthority;
          } catch {}
        });

        state.faucet.mintAuthorities = nextAuthorities;
        state.faucet.lastMintsKey = mintsKey;
        state.faucet.checked = true;
      } catch (e) {
        if (!silent) {
          const statusEl = $("faucetStatus");
          statusEl.className = "faucet-status bad";
          statusEl.textContent = `Faucet metadata check failed: ${shortKey(e.message || String(e))}`;
        }
      } finally {
        state.faucet.checking = false;
        renderFaucetStatus();
      }
    }

    async function requestDevnetAirdrop(label = "") {
      initSolanaContext();
      if (!state.wallet.publicKey) {
        renderFaucetStatus();
        return;
      }
      if (state.faucet.airdropBusy) return;

      state.faucet.airdropBusy = true;
      renderFaucetStatus();
      let okMessage = "";
      try {
        const sig = await state.connection.requestAirdrop(
          state.wallet.publicKey,
          window.solanaWeb3.LAMPORTS_PER_SOL
        );
        const latest = await state.connection.getLatestBlockhash("confirmed");
        const confirm = await state.connection.confirmTransaction(
          {
            signature: sig,
            blockhash: latest.blockhash,
            lastValidBlockHeight: latest.lastValidBlockHeight
          },
          "confirmed"
        );
        if (confirm?.value?.err) {
          throw new Error(JSON.stringify(confirm.value.err));
        }

        okMessage = `${label ? `${label}: ` : ""}1 SOL devnet airdrop confirmed (${shortKey(sig)}).`;
        const statusEl = $("faucetStatus");
        statusEl.className = "faucet-status ok";
        statusEl.textContent = okMessage;
      } catch (e) {
        const statusEl = $("faucetStatus");
        statusEl.className = "faucet-status bad";
        statusEl.textContent = `Airdrop failed: ${shortKey(e.message || String(e))}`;
      } finally {
        state.faucet.airdropBusy = false;
        if (okMessage) {
          setTimeout(renderFaucetStatus, 4000);
        } else {
          renderFaucetStatus();
        }
      }
    }

    async function requestDevnetTokens(collateralIndex) {
      const statusEl = $("faucetStatus");
      try {
        initSolanaContext();
      } catch (e) {
        statusEl.className = "faucet-status bad";
        statusEl.textContent = "Solana Web3 not loaded. Refresh the page.";
        console.error("[faucet] initSolanaContext failed:", e);
        return;
      }
      const w3 = window.solanaWeb3;
      if (!state.wallet.publicKey) {
        renderFaucetStatus();
        return;
      }
      if (state.faucet.airdropBusy) return;

      const labels = { 0: "USDC", 1: "USDT", 2: "DAI" };
      const label = labels[collateralIndex] || "Token";
      // 1000 USDC (6d), 1000 USDT (6d), 10 DAI (18d — u64-safe)
      const amounts = { 0: BigInt(1000_000_000), 1: BigInt(1000_000_000), 2: BigInt("10000000000000000000") };
      const displayAmounts = { 0: "1,000 USDC", 1: "1,000 USDT", 2: "10 DAI" };

      // Pre-check: collateral mint must be known
      const mintAddr = state.collateralMints[collateralIndex];
      if (!mintAddr) {
        statusEl.className = "faucet-status bad";
        statusEl.textContent = `Collateral mint #${collateralIndex} (${label}) not loaded yet. Wait for protocol data.`;
        return;
      }

      state.faucet.airdropBusy = true;
      renderFaucetStatus();

      statusEl.className = "faucet-status warn";
      statusEl.textContent = `Minting ${displayAmounts[collateralIndex]} to your wallet...`;

      let okMessage = "";
      try {
        console.log("[faucet] Starting mint:", label, "collateralIndex:", collateralIndex, "mint:", mintAddr);

        const faucetKp = w3.Keypair.fromSecretKey(Uint8Array.from(FAUCET_CONFIG.faucetKeypair));
        const mint = new w3.PublicKey(mintAddr);
        const owner = state.wallet.publicKey;

        console.log("[faucet] faucet pubkey:", faucetKp.publicKey.toBase58());
        console.log("[faucet] owner:", owner.toBase58());

        // Derive ATA for the user's wallet
        const ata = deriveAtaAddress(owner, mint);
        console.log("[faucet] ATA:", ata.toBase58());

        // Build transaction: Create ATA (idempotent) + MintTo
        const tx = new w3.Transaction();

        // Create ATA if it doesn't exist (idempotent instruction)
        tx.add(createAtaIdempotentIx(faucetKp.publicKey, ata, owner, mint));

        // SPL Token MintTo instruction (instruction index 7)
        const amountBigInt = amounts[collateralIndex];
        const mintToData = new Uint8Array(9);
        mintToData[0] = 7; // MintTo instruction index
        // Write u64 little-endian manually (more compatible than setBigUint64)
        let v = amountBigInt;
        for (let i = 1; i <= 8; i++) {
          mintToData[i] = Number(v & 0xFFn);
          v >>= 8n;
        }

        tx.add(new w3.TransactionInstruction({
          programId: state.pubkeys.tokenProgram,
          keys: [
            { pubkey: mint, isSigner: false, isWritable: true },
            { pubkey: ata, isSigner: false, isWritable: true },
            { pubkey: faucetKp.publicKey, isSigner: true, isWritable: false }
          ],
          data: mintToData
        }));

        // Faucet keypair pays for gas and signs as mint authority
        const latest = await state.connection.getLatestBlockhash("confirmed");
        tx.recentBlockhash = latest.blockhash;
        tx.lastValidBlockHeight = latest.lastValidBlockHeight;
        tx.feePayer = faucetKp.publicKey;
        tx.sign(faucetKp);

        console.log("[faucet] Sending transaction...");
        const sig = await state.connection.sendRawTransaction(tx.serialize(), {
          skipPreflight: false,
          preflightCommitment: "confirmed"
        });
        console.log("[faucet] TX sent:", sig);

        statusEl.textContent = `TX sent. Confirming ${label}... (${shortKey(sig)})`;

        const confirm = await state.connection.confirmTransaction(
          { signature: sig, blockhash: latest.blockhash, lastValidBlockHeight: latest.lastValidBlockHeight },
          "confirmed"
        );
        if (confirm?.value?.err) {
          throw new Error("TX confirmed with error: " + JSON.stringify(confirm.value.err));
        }

        okMessage = `✅ ${displayAmounts[collateralIndex]} minted to your wallet! (${shortKey(sig)})`;
        statusEl.className = "faucet-status ok";
        statusEl.textContent = okMessage;
        console.log("[faucet] SUCCESS:", sig);

        // Refresh balances after successful mint
        setTimeout(() => refreshWalletBalances({ silent: true }), 2000);
      } catch (e) {
        const msg = e.message || String(e);
        console.error("[faucet] FAILED:", msg, e);
        statusEl.className = "faucet-status bad";
        if (msg.includes("0x1") || msg.includes("insufficient")) {
          statusEl.textContent = `❌ Mint failed: faucet wallet needs SOL for gas. Try again later.`;
        } else if (msg.includes("expired") || msg.includes("blockhash") || msg.includes("block height")) {
          // TX was likely sent successfully but confirmation timed out
          statusEl.className = "faucet-status ok";
          statusEl.textContent = `⚠️ TX sent but confirmation timed out. Tokens likely arrived — check your balance!`;
          okMessage = statusEl.textContent;
          setTimeout(() => refreshWalletBalances({ silent: true }), 3000);
        } else {
          statusEl.textContent = `❌ Mint failed: ${msg.substring(0, 120)}`;
        }
      } finally {
        state.faucet.airdropBusy = false;
        if (okMessage) {
          setTimeout(renderFaucetStatus, 5000);
        } else {
          setTimeout(renderFaucetStatus, 8000);
        }
      }
    }

    function setWalletPublicKey(pubkeyLike) {
      if (!pubkeyLike) {
        state.wallet.publicKey = null;
        state.wallet.balances = { USDC: 0, USDT: 0, DAI: 0, MSTB: 0 };
      } else {
        state.wallet.publicKey = new window.solanaWeb3.PublicKey(pubkeyLike.toString());
      }
      renderWalletControls();
      renderWalletBalances();
      renderFaucetStatus();
      updateMintEstimate();
      updateRedeemEstimate();
      updateMintButton();
      updateRedeemButton();
      updateAgentRegisterButton();
    }

    async function refreshWalletBalances(opts = {}) {
      const silent = !!opts.silent;
      if (!state.wallet.publicKey || state.wallet.balanceBusy) {
        return;
      }
      state.wallet.balanceBusy = true;
      try {
        const owner = state.wallet.publicKey.toBase58();
        console.log("[wallet] Refreshing balances for:", owner);
        const tokenAccounts = await rpc(
          "getTokenAccountsByOwner",
          [owner, { programId: CFG.TOKEN_PROGRAM }, { encoding: "jsonParsed" }],
          12000
        );

        const mintToAmount = new Map();
        const values = tokenAccounts?.value || [];
        console.log("[wallet] Found", values.length, "token accounts");
        for (const item of values) {
          const info = item?.account?.data?.parsed?.info;
          const mint = info?.mint;
          const amount = Number(info?.tokenAmount?.uiAmountString ?? info?.tokenAmount?.uiAmount ?? 0);
          if (mint) {
            mintToAmount.set(mint, amount);
            console.log("[wallet]   mint:", mint.substring(0, 8) + "...", "amount:", amount);
          }
        }

        const mintUSDC = state.collateralMints[0];
        const mintUSDT = state.collateralMints[1];
        const mintDAI = state.collateralMints[2];

        if (!mintUSDC) console.warn("[wallet] USDC mint address not loaded yet");

        // Also fetch SOL balance
        let solBalance = 0;
        try {
          const lamports = await state.connection.getBalance(state.wallet.publicKey, "confirmed");
          solBalance = lamports / 1e9;
        } catch (e2) {
          console.warn("[wallet] SOL balance fetch failed:", e2.message);
        }

        state.wallet.balances = {
          SOL: solBalance,
          USDC: Number(mintToAmount.get(mintUSDC) || 0),
          USDT: Number(mintToAmount.get(mintUSDT) || 0),
          DAI: Number(mintToAmount.get(mintDAI) || 0),
          MSTB: Number(mintToAmount.get(CFG.MSTB_MINT) || 0)
        };
        console.log("[wallet] Balances:", JSON.stringify(state.wallet.balances));

        renderWalletBalances();
        renderFaucetStatus();
        updateMintEstimate();
        updateRedeemEstimate();
        updateAgentRegisterButton();
      } catch (e) {
        console.error("[wallet] Balance refresh failed:", e);
        if (!silent) setMintTxStatus("error", `Balance refresh failed: ${shortKey(e.message || String(e))}`);
      } finally {
        state.wallet.balanceBusy = false;
      }
    }

    async function connectWallet() {
      if (!state.wallet.provider) {
        window.open("https://phantom.app/", "_blank", "noopener,noreferrer");
        return;
      }
      try {
        setMintTxStatus("muted", "Connecting Phantom wallet...");
        setRedeemTxStatus("muted", "Connecting Phantom wallet...");
        setAgentRegisterStatus("muted", "Connecting Phantom wallet...");
        const resp = await state.wallet.provider.connect();
        setWalletPublicKey(resp?.publicKey || state.wallet.provider.publicKey);
        await refreshWalletBalances({ silent: true });
        setMintTxStatus("ok", "Wallet connected.");
        setRedeemTxStatus("ok", "Wallet connected.");
        setAgentRegisterStatus("ok", "Wallet connected.");
      } catch (e) {
        const msg = `Wallet connect failed: ${shortKey(e.message || String(e))}`;
        setMintTxStatus("error", msg);
        setRedeemTxStatus("error", msg);
        setAgentRegisterStatus("error", msg);
      }
    }

    async function disconnectWallet() {
      if (!state.wallet.provider) return;
      try {
        await state.wallet.provider.disconnect();
      } catch {}
      setWalletPublicKey(null);
      setMintTxStatus("muted", "Wallet disconnected.");
      setRedeemTxStatus("muted", "Wallet disconnected.");
      setAgentRegisterStatus("muted", "Wallet disconnected.");
    }

    function selectedMintCollateralIndex() {
      return Number($("mintCollateral")?.value || "0");
    }

    function selectedMintCollateralSymbol() {
      return COLLATERAL_META[selectedMintCollateralIndex()]?.symbol || "USDC";
    }

    function selectedMintCollateralDecimals() {
      return COLLATERAL_META[selectedMintCollateralIndex()]?.decimals || 6;
    }

    function selectedMintCollateralBalance() {
      const symbol = selectedMintCollateralSymbol();
      return Number(state.wallet.balances?.[symbol] || 0);
    }

    function selectedRedeemCollateralIndex() {
      return Number($("redeemCollateral")?.value || "0");
    }

    function selectedRedeemCollateralSymbol() {
      return COLLATERAL_META[selectedRedeemCollateralIndex()]?.symbol || "USDC";
    }

    function vaultByIndex(index) {
      return (state.lastVaults || []).find((v) => Number(v.index) === Number(index));
    }

    function setStatusWithSignature(elId, kind, text, signature = "") {
      const el = $(elId);
      if (!el) return;
      const cls = kind === "ok" ? "ok" : kind === "warn" ? "warn" : kind === "error" ? "bad" : "muted";
      el.className = `mint-status ${cls}`;
      el.replaceChildren();
      el.appendChild(document.createTextNode(text));

      if (signature) {
        const spacer = document.createTextNode(" ");
        const link = document.createElement("a");
        link.href = `https://explorer.solana.com/tx/${signature}?cluster=devnet`;
        link.target = "_blank";
        link.rel = "noreferrer";
        link.textContent = shortKey(signature);
        el.append(spacer, link);
      }
    }

    function setMintTxStatus(kind, text, signature = "") {
      setStatusWithSignature("mintTxStatus", kind, text, signature);
    }

    function setRedeemTxStatus(kind, text, signature = "") {
      setStatusWithSignature("redeemTxStatus", kind, text, signature);
    }

    function setAgentRegisterStatus(kind, text, signature = "") {
      setStatusWithSignature("agentRegisterStatus", kind, text, signature);
    }

    function updateMintEstimate() {
      const idx = selectedMintCollateralIndex();
      const dec = selectedMintCollateralDecimals();
      const amountRaw = parseUiAmountToRaw($("mintAmount")?.value || "", dec);
      const vault = vaultByIndex(idx);
      const feeRate = Number(state.lastProtocol?.mint_fee_rate || 0);

      if ($("mintFeeMeta")) $("mintFeeMeta").textContent = `Fee: ${ppmToPct(feeRate).toFixed(4)}%`;
      if (vault?.price) {
        if ($("mintPriceMeta")) $("mintPriceMeta").textContent = `Oracle price: $${(Number(vault.price) / 1e6).toFixed(6)}`;
      } else {
        if ($("mintPriceMeta")) $("mintPriceMeta").textContent = "Oracle price: --";
      }

      if (amountRaw === null || amountRaw <= 0n || !vault?.price) {
        if ($("mintEstimate")) $("mintEstimate").textContent = "--";
        updateMintButton();
        return;
      }

      const gross = (amountRaw * vault.price) / 1000000n;
      const feeScaler = BigInt(Math.max(0, 1_000_000 - feeRate));
      const net = (gross * feeScaler) / 1000000n;
      if ($("mintEstimate")) $("mintEstimate").textContent = fmtToken(net, 6, 6);
      updateMintButton();
    }

    function buildRedeemPreview(musdAmountRaw) {
      const protocol = state.lastProtocol;
      if (!protocol || musdAmountRaw <= 0n) return null;

      const supplyBefore = protocol.total_supply || 0n;
      if (supplyBefore <= 0n) {
        return {
          feeRate: Number(protocol.redeem_fee_rate || 0),
          totalOut: 0n,
          payoutByIndex: { 0: 0n, 1: 0n, 2: 0n, 3: 0n }
        };
      }

      const feeRate = Number(protocol.redeem_fee_rate || 0);
      const redeemFee = (musdAmountRaw * BigInt(Math.max(0, feeRate))) / 1000000n;
      const netRedeem = musdAmountRaw > redeemFee ? musdAmountRaw - redeemFee : 0n;

      const payoutByIndex = { 0: 0n, 1: 0n, 2: 0n, 3: 0n };
      let totalOut = 0n;
      for (const idx of [0, 1, 2, 3]) {
        const vault = vaultByIndex(idx);
        const deposits = vault?.total_deposits || 0n;
        if (deposits <= 0n || netRedeem <= 0n) continue;
        const payout = (deposits * netRedeem) / supplyBefore;
        payoutByIndex[idx] = payout;
        totalOut += payout;
      }

      return { feeRate, totalOut, payoutByIndex };
    }

    function updateRedeemEstimate() {
      if (!$("redeemEstimate")) return;
      const idx = selectedRedeemCollateralIndex();
      const musdRaw = parseUiAmountToRaw($("redeemAmount")?.value || "", 6);
      const preview = musdRaw !== null && musdRaw > 0n ? buildRedeemPreview(musdRaw) : null;
      const selectedVault = vaultByIndex(idx);

      const feeRate = Number(state.lastProtocol?.redeem_fee_rate || 0);
      $("redeemFeeMeta").textContent = `Fee: ${ppmToPct(feeRate).toFixed(4)}%`;
      $("redeemPriceMeta").textContent = selectedVault?.price
        ? `Oracle price: $${(Number(selectedVault.price) / 1e6).toFixed(6)}`
        : "Oracle price: --";

      if (!preview) {
        $("redeemEstimate").textContent = "--";
        if ($("redeemBasketMeta")) $("redeemBasketMeta").textContent = "Basket payout: --";
        if ($("redeemOutUSDC")) $("redeemOutUSDC").textContent = "--";
        if ($("redeemOutUSDT")) $("redeemOutUSDT").textContent = "--";
        if ($("redeemOutDAI")) $("redeemOutDAI").textContent = "--";
        updateRedeemButton();
        return;
      }

      const selectedOut = preview.payoutByIndex[idx] || 0n;
      $("redeemEstimate").textContent = fmtToken(selectedOut, 6, 6);
      if ($("redeemBasketMeta")) {
        $("redeemBasketMeta").textContent = `Basket payout: ${fmtToken(preview.totalOut, 6, 6)} tokens (all vault legs)`;
      }
      if ($("redeemOutUSDC")) $("redeemOutUSDC").textContent = fmtToken(preview.payoutByIndex[0] || 0n, 6, 6);
      if ($("redeemOutUSDT")) $("redeemOutUSDT").textContent = fmtToken(preview.payoutByIndex[1] || 0n, 6, 6);
      if ($("redeemOutDAI")) $("redeemOutDAI").textContent = fmtToken(preview.payoutByIndex[2] || 0n, 6, 6);
      updateRedeemButton();
    }

    function updateMintButton() {
      const btn = $("mintSubmitBtn");
      if (!btn) return;
      const dec = selectedMintCollateralDecimals();
      const amountRaw = parseUiAmountToRaw($("mintAmount")?.value || "", dec);
      const mintKnown = !!state.collateralMints[selectedMintCollateralIndex()];
      const balanceRaw = walletBalanceRaw(selectedMintCollateralSymbol(), dec);
      const sufficient = amountRaw !== null && amountRaw <= balanceRaw;
      const ready = !!state.wallet.publicKey
        && amountRaw !== null
        && amountRaw > 0n
        && mintKnown
        && sufficient
        && !state.mintBusy;
      btn.disabled = !ready;
      btn.textContent = state.mintBusy ? "MINTING..." : "MINT";
    }

    function updateRedeemButton() {
      const btn = $("redeemSubmitBtn");
      if (!btn) return;
      const amountRaw = parseUiAmountToRaw($("redeemAmount")?.value || "", 6);
      const mintsReady = !!state.collateralMints[0] && !!state.collateralMints[1] && !!state.collateralMints[2] && !!state.collateralMints[3];
      const mstbRaw = walletBalanceRaw("MSTB", 6);
      const sufficient = amountRaw !== null && amountRaw <= mstbRaw;
      const ready = !!state.wallet.publicKey
        && amountRaw !== null
        && amountRaw > 0n
        && mintsReady
        && sufficient
        && !state.redeemBusy;
      btn.disabled = !ready;
      btn.textContent = state.redeemBusy ? "REDEEMING..." : "REDEEM";
    }

    function updateAgentStakeHint() {
      const role = Number($("agentRole")?.value || "0");
      const recommended = AGENT_ROLE_MIN_STAKE_SOL[role] || 1;
      const hint = $("agentStakeHint");
      if (hint) {
        hint.textContent = `Recommended stake (${ROLE_MAP[role] || "Agent"}): ${recommended} SOL • on-chain min: 1 SOL`;
      }
    }

    function updateAgentRegisterButton() {
      const btn = $("agentRegisterBtn");
      if (!btn) return;
      const stakeRaw = parseUiAmountToRaw($("agentStake")?.value || "", 9);
      const role = Number($("agentRole")?.value || "0");
      const validRole = Number.isFinite(role) && role >= 0 && role <= 3;
      const ready = !!state.wallet.publicKey
        && validRole
        && stakeRaw !== null
        && stakeRaw > 0n
        && !state.registerBusy;
      btn.disabled = !ready;
      btn.textContent = state.registerBusy ? "REGISTERING..." : "REGISTER AGENT";
    }

    function renderAgentRegistryPreview(records) {
      const body = $("agentRegistryRows");
      if (!body) return;
      body.replaceChildren();

      if (!records?.length) {
        body.appendChild(createPlaceholderRow(5, "No on-chain agent registrations yet."));
        return;
      }

      const rows = [...records]
        .sort((a, b) => Number(b.registered_slot - a.registered_slot))
        .slice(0, 8);

      rows.forEach((r) => {
        const tr = document.createElement("tr");
        const agentCell = document.createElement("td");
        agentCell.textContent = shortKey(r.agent);
        const roleCell = document.createElement("td");
        roleCell.textContent = roleLabel(r.role);
        const stakeCell = document.createElement("td");
        stakeCell.textContent = `${(Number(r.stake || 0n) / 1e9).toFixed(3)} SOL`;
        const statusCell = document.createElement("td");
        statusCell.textContent = statusLabel(r.status);
        const scoreCell = document.createElement("td");
        scoreCell.textContent = Number(r.agent_score || 0n).toLocaleString();
        tr.append(agentCell, roleCell, stakeCell, statusCell, scoreCell);
        body.appendChild(tr);
      });
    }

    async function submitMint() {
      initSolanaContext();
      if (!state.wallet.publicKey) {
        setMintTxStatus("warn", "Connect wallet first.");
        return;
      }

      const collateralIndex = selectedMintCollateralIndex();
      const collateralMintStr = state.collateralMints[collateralIndex];
      if (!collateralMintStr) {
        setMintTxStatus("error", "Collateral mint not resolved yet. Wait for next refresh.");
        return;
      }

      const collateralDecimals = selectedMintCollateralDecimals();
      const collateralAmountRaw = parseUiAmountToRaw($("mintAmount")?.value || "", collateralDecimals);
      if (collateralAmountRaw === null || collateralAmountRaw <= 0n) {
        setMintTxStatus("warn", "Enter a valid mint amount.");
        return;
      }

      const balanceRaw = walletBalanceRaw(selectedMintCollateralSymbol(), collateralDecimals);
      if (collateralAmountRaw > balanceRaw) {
        setMintTxStatus("warn", "Insufficient collateral balance.");
        return;
      }

      const selectedVault = vaultByIndex(collateralIndex);

      state.mintBusy = true;
      updateMintButton();
      try {
        const w3 = window.solanaWeb3;
        const user = state.wallet.publicKey;
        const collateralMint = new w3.PublicKey(collateralMintStr);

        const [userPosition] = w3.PublicKey.findProgramAddressSync(
          [new TextEncoder().encode("user_position"), user.toBytes()],
          state.pubkeys.programId
        );

        const userCollateralAta = deriveAtaAddress(user, collateralMint);
        const vaultCollateralAta = deriveAtaAddress(state.pubkeys.protocolState, collateralMint);
        const userMstbAta = deriveAtaAddress(user, state.pubkeys.mstbMint);

        const [userCollateralInfo, vaultCollateralInfo, userMstbInfo] = await state.connection.getMultipleAccountsInfo([
          userCollateralAta,
          vaultCollateralAta,
          userMstbAta
        ]);

        const tx = new w3.Transaction();
        if (!userCollateralInfo) tx.add(createAtaIdempotentIx(user, userCollateralAta, user, collateralMint));
        if (!vaultCollateralInfo) tx.add(createAtaIdempotentIx(user, vaultCollateralAta, state.pubkeys.protocolState, collateralMint));
        if (!userMstbInfo) tx.add(createAtaIdempotentIx(user, userMstbAta, user, state.pubkeys.mstbMint));

        const discriminator = await getMintDiscriminator();
        // Devnet: bypass MintPriceAboveUserLimit by setting max_price to u64::MAX
        const maxPriceRaw = 18446744073709551615n; // u64::MAX — no slippage limit on devnet
        const ixData = encodeMintInstructionData(collateralIndex, collateralAmountRaw, maxPriceRaw, discriminator);

        tx.add(new w3.TransactionInstruction({
          programId: state.pubkeys.programId,
          keys: [
            { pubkey: state.pubkeys.protocolState, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.circuitBreaker, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[0], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[1], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[2], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[3], isSigner: false, isWritable: true },
            { pubkey: user, isSigner: true, isWritable: true },
            { pubkey: userPosition, isSigner: false, isWritable: true },
            { pubkey: userCollateralAta, isSigner: false, isWritable: true },
            { pubkey: vaultCollateralAta, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.mstbMint, isSigner: false, isWritable: true },
            { pubkey: userMstbAta, isSigner: false, isWritable: true },
            { pubkey: collateralMint, isSigner: false, isWritable: false },
            { pubkey: state.pubkeys.tokenProgram, isSigner: false, isWritable: false },
            { pubkey: state.pubkeys.associatedTokenProgram, isSigner: false, isWritable: false },
            { pubkey: state.pubkeys.systemProgram, isSigner: false, isWritable: false }
          ],
          data: ixData
        }));

        const latest = await state.connection.getLatestBlockhash("confirmed");
        tx.recentBlockhash = latest.blockhash;
        tx.feePayer = user;

        console.log("[submitMint] collateral:", collateralIndex, "amount:", collateralAmountRaw.toString(), "maxPrice:", maxPriceRaw.toString());
        console.log("[submitMint] accounts:", {
          protocolState: state.pubkeys.protocolState.toBase58(),
          user: user.toBase58(),
          userCollateralAta: userCollateralAta.toBase58(),
          vaultCollateralAta: vaultCollateralAta.toBase58(),
          userMstbAta: userMstbAta.toBase58(),
          collateralMint: collateralMint.toBase58(),
          userPosition: userPosition.toBase58()
        });

        // Pre-flight simulation to catch program errors with full logs
        setMintTxStatus("warn", "Simulating transaction...");
        try {
          const simResult = await state.connection.simulateTransaction(tx, { sigVerify: false });
          if (simResult?.value?.err) {
            const simLogs = simResult.value.logs || [];
            console.error("[submitMint] Simulation failed:", JSON.stringify(simResult.value.err));
            console.error("[submitMint] Simulation logs:", simLogs);
            const errDetail = JSON.stringify(simResult.value.err);
            const lastLogLine = simLogs.filter(l => l.includes("Error") || l.includes("failed") || l.includes("Program log")).pop() || "";
            throw new Error(`Simulation failed: ${errDetail}. ${lastLogLine}`);
          }
          console.log("[submitMint] Simulation OK, CU:", simResult?.value?.unitsConsumed);
        } catch (simErr) {
          if (simErr.message?.startsWith("Simulation failed:")) throw simErr;
          console.warn("[submitMint] Simulation check skipped:", simErr.message);
        }

        setMintTxStatus("warn", "Requesting wallet signature...");
        let signature;
        try {
          const sendResult = await state.wallet.provider.signAndSendTransaction(tx, {
            preflightCommitment: "confirmed"
          });
          signature = typeof sendResult === "string" ? sendResult : sendResult?.signature;
        } catch (walletErr) {
          console.warn("[submitMint] signAndSendTransaction failed:", walletErr.message);
          if (state.wallet.provider.signTransaction) {
            const signed = await state.wallet.provider.signTransaction(tx);
            signature = await state.connection.sendRawTransaction(signed.serialize(), {
              skipPreflight: false,
              preflightCommitment: "confirmed"
            });
          } else {
            throw walletErr;
          }
        }
        if (!signature) throw new Error("No signature returned by wallet");

        setMintTxStatus("warn", "Mint pending:", signature);
        const confirm = await state.connection.confirmTransaction(
          {
            signature,
            blockhash: latest.blockhash,
            lastValidBlockHeight: latest.lastValidBlockHeight
          },
          "confirmed"
        );

        if (confirm?.value?.err) {
          throw new Error(JSON.stringify(confirm.value.err));
        }

        setMintTxStatus("ok", "Mint confirmed:", signature);
        $("mintAmount").value = "";
        updateMintEstimate();
        await refreshWalletBalances({ silent: true });
        await poll();
      } catch (e) {
        const msg = e.message || String(e);
        console.error("[submitMint] FAILED:", msg, e);
        if (e.logs) console.error("[submitMint] Program logs:", e.logs);
        // Block height expired = TX was sent, likely succeeded
        if (msg.includes("expired") || msg.includes("block height")) {
          setMintTxStatus("warn", "TX sent but confirmation timed out. Check your balance!");
          setTimeout(() => refreshWalletBalances({ silent: true }), 3000);
          state.mintBusy = false;
          updateMintButton();
          return;
        }
        // Show full error, not shortKey'd
        let displayMsg = msg;
        // Try to extract Anchor error code
        const anchorMatch = msg.match(/custom program error:\s*0x([0-9a-fA-F]+)/i);
        if (anchorMatch) {
          const code = parseInt(anchorMatch[1], 16);
          const anchorErrors = {
            6000: "Unauthorized", 6016: "InvalidCollateralIndex", 6017: "InvalidAmount",
            6018: "MathOverflow", 6021: "InvalidCrTarget", 6023: "OracleStale",
            6024: "ConfidenceTooHigh", 6033: "InvalidPrice",
            6034: "InsufficientCollateralRatio", 6035: "MintPausedByCircuitBreaker",
            6036: "MintRateLimited", 6037: "MintPriceAboveUserLimit (slippage exceeded)",
            6038: "MintTxFlowLimitExceeded", 6039: "MintSlotFlowLimitExceeded",
            6043: "DepegMintPaused", 6054: "OracleDegraded",
            6064: "EmergencyShutdownActive", 6067: "AmountTooLarge"
          };
          displayMsg = `Program error 0x${anchorMatch[1]}: ${anchorErrors[code] || 'Unknown'} — ${msg.substring(0, 100)}`;
        } else if (msg.length > 150) {
          displayMsg = msg.substring(0, 150) + "...";
        }
        setMintTxStatus("error", `Mint failed: ${displayMsg}`);
      } finally {
        state.mintBusy = false;
        updateMintButton();
      }
    }

    async function submitRedeem() {
      initSolanaContext();
      if (!state.wallet.publicKey) {
        setRedeemTxStatus("warn", "Connect wallet first.");
        return;
      }

      const musdAmountRaw = parseUiAmountToRaw($("redeemAmount")?.value || "", 6);
      if (musdAmountRaw === null || musdAmountRaw <= 0n) {
        setRedeemTxStatus("warn", "Enter a valid MSTB amount.");
        return;
      }

      const mstbRaw = walletBalanceRaw("MSTB", 6);
      if (musdAmountRaw > mstbRaw) {
        setRedeemTxStatus("warn", "Insufficient MSTB balance.");
        return;
      }

      if (!state.collateralMints[0] || !state.collateralMints[1] || !state.collateralMints[2] || !state.collateralMints[3]) {
        setRedeemTxStatus("error", "Collateral mints are not fully resolved yet.");
        return;
      }

      state.redeemBusy = true;
      updateRedeemButton();
      try {
        const w3 = window.solanaWeb3;
        const user = state.wallet.publicKey;
        const te = new TextEncoder();

        const [userPosition] = w3.PublicKey.findProgramAddressSync(
          [te.encode("user_position"), user.toBytes()],
          state.pubkeys.programId
        );
        const userPositionInfo = await state.connection.getAccountInfo(userPosition, "confirmed");
        if (!userPositionInfo) {
          throw new Error("User position not found. Mint first from this wallet.");
        }

        const usdcMint = new w3.PublicKey(state.collateralMints[0]);
        const usdtMint = new w3.PublicKey(state.collateralMints[1]);
        const daiMint = new w3.PublicKey(state.collateralMints[2]);
        const usdsMint = new w3.PublicKey(state.collateralMints[3]);

        const userUsdcAta = deriveAtaAddress(user, usdcMint);
        const userUsdtAta = deriveAtaAddress(user, usdtMint);
        const userDaiAta = deriveAtaAddress(user, daiMint);
        const userUsdsAta = deriveAtaAddress(user, usdsMint);

        const vaultUsdcAta = deriveAtaAddress(state.pubkeys.protocolState, usdcMint);
        const vaultUsdtAta = deriveAtaAddress(state.pubkeys.protocolState, usdtMint);
        const vaultDaiAta = deriveAtaAddress(state.pubkeys.protocolState, daiMint);
        const vaultUsdsAta = deriveAtaAddress(state.pubkeys.protocolState, usdsMint);

        const userMstbAta = deriveAtaAddress(user, state.pubkeys.mstbMint);

        const ataAddresses = [
          userUsdcAta,
          userUsdtAta,
          userDaiAta,
          userUsdsAta,
          vaultUsdcAta,
          vaultUsdtAta,
          vaultDaiAta,
          vaultUsdsAta,
          userMstbAta
        ];
        const infos = await state.connection.getMultipleAccountsInfo(ataAddresses);

        const tx = new w3.Transaction();
        const maybeCreateAta = (info, ata, owner, mint) => {
          if (!info) tx.add(createAtaIdempotentIx(user, ata, owner, mint));
        };

        maybeCreateAta(infos[0], userUsdcAta, user, usdcMint);
        maybeCreateAta(infos[1], userUsdtAta, user, usdtMint);
        maybeCreateAta(infos[2], userDaiAta, user, daiMint);
        maybeCreateAta(infos[3], userUsdsAta, user, usdsMint);
        maybeCreateAta(infos[4], vaultUsdcAta, state.pubkeys.protocolState, usdcMint);
        maybeCreateAta(infos[5], vaultUsdtAta, state.pubkeys.protocolState, usdtMint);
        maybeCreateAta(infos[6], vaultDaiAta, state.pubkeys.protocolState, daiMint);
        maybeCreateAta(infos[7], vaultUsdsAta, state.pubkeys.protocolState, usdsMint);
        maybeCreateAta(infos[8], userMstbAta, user, state.pubkeys.mstbMint);

        const preview = buildRedeemPreview(musdAmountRaw);
        const minOutAmountRaw = preview ? (preview.totalOut * 60n) / 100n : 0n;

        const discriminator = await getRedeemDiscriminator();
        const ixData = encodeRedeemInstructionData(musdAmountRaw, minOutAmountRaw, discriminator);
        tx.add(new w3.TransactionInstruction({
          programId: state.pubkeys.programId,
          keys: [
            { pubkey: state.pubkeys.protocolState, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.circuitBreaker, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[0], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[1], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[2], isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.vaultPdas[3], isSigner: false, isWritable: true },
            { pubkey: user, isSigner: true, isWritable: true },
            { pubkey: userPosition, isSigner: false, isWritable: true },
            { pubkey: userUsdcAta, isSigner: false, isWritable: true },
            { pubkey: userUsdtAta, isSigner: false, isWritable: true },
            { pubkey: userDaiAta, isSigner: false, isWritable: true },
            { pubkey: userUsdsAta, isSigner: false, isWritable: true },
            { pubkey: vaultUsdcAta, isSigner: false, isWritable: true },
            { pubkey: vaultUsdtAta, isSigner: false, isWritable: true },
            { pubkey: vaultDaiAta, isSigner: false, isWritable: true },
            { pubkey: vaultUsdsAta, isSigner: false, isWritable: true },
            { pubkey: usdcMint, isSigner: false, isWritable: false },
            { pubkey: usdtMint, isSigner: false, isWritable: false },
            { pubkey: daiMint, isSigner: false, isWritable: false },
            { pubkey: usdsMint, isSigner: false, isWritable: false },
            { pubkey: state.pubkeys.mstbMint, isSigner: false, isWritable: true },
            { pubkey: userMstbAta, isSigner: false, isWritable: true },
            { pubkey: state.pubkeys.tokenProgram, isSigner: false, isWritable: false },
            { pubkey: state.pubkeys.associatedTokenProgram, isSigner: false, isWritable: false }
          ],
          data: ixData
        }));

        const latest = await state.connection.getLatestBlockhash("confirmed");
        tx.recentBlockhash = latest.blockhash;
        tx.feePayer = user;

        setRedeemTxStatus("warn", "Transaction submitted... awaiting signature.");
        const sendResult = await state.wallet.provider.signAndSendTransaction(tx, {
          preflightCommitment: "confirmed"
        });
        const signature = typeof sendResult === "string" ? sendResult : sendResult?.signature;
        if (!signature) throw new Error("No signature returned by wallet");

        setRedeemTxStatus("warn", "Redeem pending:", signature);
        const confirm = await state.connection.confirmTransaction(
          {
            signature,
            blockhash: latest.blockhash,
            lastValidBlockHeight: latest.lastValidBlockHeight
          },
          "confirmed"
        );

        if (confirm?.value?.err) {
          throw new Error(JSON.stringify(confirm.value.err));
        }

        setRedeemTxStatus("ok", "Redeem confirmed:", signature);
        $("redeemAmount").value = "";
        updateRedeemEstimate();
        await refreshWalletBalances({ silent: true });
        await poll();
      } catch (e) {
        const rmsg = e.message || String(e);
        console.error("[submitRedeem] FAILED:", rmsg, e);
        setRedeemTxStatus("error", `Redeem failed: ${rmsg.substring(0, 150)}`);
      } finally {
        state.redeemBusy = false;
        updateRedeemButton();
      }
    }

    async function submitAgentRegistration() {
      initSolanaContext();
      if (!state.wallet.publicKey) {
        setAgentRegisterStatus("warn", "Connect wallet first.");
        return;
      }

      const role = Number($("agentRole")?.value || "0");
      if (!Number.isFinite(role) || role < 0 || role > 3) {
        setAgentRegisterStatus("warn", "Select a valid agent type.");
        return;
      }

      const stakeLamportsRaw = parseUiAmountToRaw($("agentStake")?.value || "", 9);
      if (stakeLamportsRaw === null || stakeLamportsRaw <= 0n) {
        setAgentRegisterStatus("warn", "Enter a valid stake amount (SOL).");
        return;
      }
      if (stakeLamportsRaw < 1_000_000_000n) {
        setAgentRegisterStatus("warn", "Minimum on-chain stake is 1 SOL.");
        return;
      }

      state.registerBusy = true;
      updateAgentRegisterButton();
      try {
        const w3 = window.solanaWeb3;
        const user = state.wallet.publicKey;
        const te = new TextEncoder();

        const [agentRecordPda] = w3.PublicKey.findProgramAddressSync(
          [te.encode("agent"), user.toBytes()],
          state.pubkeys.programId
        );
        const [agentEscrowV2] = w3.PublicKey.findProgramAddressSync(
          [te.encode("v2:agent_escrow"), user.toBytes()],
          state.pubkeys.programId
        );
        const [agentEscrowLegacyWallet] = w3.PublicKey.findProgramAddressSync(
          [te.encode("agent_escrow"), user.toBytes()],
          state.pubkeys.programId
        );
        const [agentEscrowLegacyGlobal] = w3.PublicKey.findProgramAddressSync(
          [te.encode("agent_escrow")],
          state.pubkeys.programId
        );

        const existing = await state.connection.getAccountInfo(agentRecordPda, "confirmed");
        if (existing) {
          throw new Error("Agent already registered for this wallet");
        }

        const balanceLamports = await state.connection.getBalance(user, "confirmed");
        const feeBuffer = 10_000_000n;
        if (BigInt(balanceLamports) < stakeLamportsRaw + feeBuffer) {
          throw new Error("Insufficient SOL for stake + transaction fees");
        }

        const discriminator = await getRegisterAgentDiscriminator();
        const ixData = encodeRegisterAgentInstructionData(role, stakeLamportsRaw, discriminator);

        const isEscrowSeedMismatch = (err) => {
          const msg = String(err?.message || err || "");
          return msg.includes("ConstraintSeeds") && msg.includes("agent_escrow");
        };

        const candidates = [
          { label: "v2", pda: agentEscrowV2 },
          { label: "legacy-wallet", pda: agentEscrowLegacyWallet },
          { label: "legacy-global", pda: agentEscrowLegacyGlobal }
        ];

        let finalSignature = "";
        let lastError = null;

        for (let i = 0; i < candidates.length; i++) {
          const candidate = candidates[i];
          const tx = new w3.Transaction().add(new w3.TransactionInstruction({
            programId: state.pubkeys.programId,
            keys: [
              { pubkey: user, isSigner: true, isWritable: true },
              { pubkey: agentRecordPda, isSigner: false, isWritable: true },
              { pubkey: candidate.pda, isSigner: false, isWritable: true },
              { pubkey: state.pubkeys.systemProgram, isSigner: false, isWritable: false }
            ],
            data: ixData
          }));

          const latest = await state.connection.getLatestBlockhash("confirmed");
          tx.recentBlockhash = latest.blockhash;
          tx.feePayer = user;

          try {
            setAgentRegisterStatus(
              "warn",
              i === 0
                ? "Registration submitted... awaiting signature."
                : `Retrying registration with compatible escrow PDA (${candidate.label})...`
            );
            const sendResult = await state.wallet.provider.signAndSendTransaction(tx, {
              preflightCommitment: "confirmed"
            });
            const signature = typeof sendResult === "string" ? sendResult : sendResult?.signature;
            if (!signature) throw new Error("No signature returned by wallet");

            setAgentRegisterStatus("warn", "Registration pending:", signature);
            const confirm = await state.connection.confirmTransaction(
              {
                signature,
                blockhash: latest.blockhash,
                lastValidBlockHeight: latest.lastValidBlockHeight
              },
              "confirmed"
            );

            if (confirm?.value?.err) {
              throw new Error(JSON.stringify(confirm.value.err));
            }

            finalSignature = signature;
            break;
          } catch (err) {
            lastError = err;
            const canRetry = i < candidates.length - 1;
            if (!canRetry || !isEscrowSeedMismatch(err)) break;
          }
        }

        if (!finalSignature) {
          throw lastError || new Error("Registration failed");
        }

        setAgentRegisterStatus("ok", "Agent registration confirmed:", finalSignature);
        await poll();
      } catch (e) {
        setAgentRegisterStatus("error", `Registration failed: ${shortKey(e.message || String(e))}`);
      } finally {
        state.registerBusy = false;
        updateAgentRegisterButton();
      }
    }

    function bindWalletAndActionUi() {
      initSolanaContext();

      const provider = walletProvider();
      state.wallet.provider = provider;
      renderWalletControls();
      renderWalletBalances();
      renderFaucetStatus();
      updateMintEstimate();
      updateRedeemEstimate();
      updateMintButton();
      updateRedeemButton();
      updateAgentStakeHint();
      updateAgentRegisterButton();

      $("walletConnectBtn").addEventListener("click", connectWallet);
      $("walletDisconnectBtn").addEventListener("click", disconnectWallet);

      $("mintMaxBtn").addEventListener("click", () => {
        const bal = selectedMintCollateralBalance();
        $("mintAmount").value = uiNumberToTrimmedText(bal);
        updateMintEstimate();
      });
      $("mintAmount").addEventListener("input", updateMintEstimate);
      $("mintCollateral").addEventListener("change", updateMintEstimate);
      $("mintSubmitBtn").addEventListener("click", submitMint);

      if ($("redeemMaxBtn")) {
        $("redeemMaxBtn").addEventListener("click", () => {
          $("redeemAmount").value = uiNumberToTrimmedText(state.wallet.balances?.MSTB || 0);
          updateRedeemEstimate();
        });
      }
      if ($("redeemAmount")) $("redeemAmount").addEventListener("input", updateRedeemEstimate);
      if ($("redeemCollateral")) $("redeemCollateral").addEventListener("change", updateRedeemEstimate);
      if ($("redeemSubmitBtn")) $("redeemSubmitBtn").addEventListener("click", submitRedeem);

      if ($("agentRole")) $("agentRole").addEventListener("change", () => {
        updateAgentStakeHint();
        updateAgentRegisterButton();
      });
      if ($("agentStake")) $("agentStake").addEventListener("input", updateAgentRegisterButton);
      if ($("agentRegisterBtn")) $("agentRegisterBtn").addEventListener("click", submitAgentRegistration);

      if ($("faucetSolBtn")) {
        $("faucetSolBtn").addEventListener("click", () => requestDevnetAirdrop("SOL"));
      }
      [
        ["faucetUsdcBtn", 0],
        ["faucetUsdtBtn", 1],
        ["faucetDaiBtn", 2]
      ].forEach(([id, idx]) => {
        $(id).addEventListener("click", () => {
          if (FAUCET_CONFIG.instructionAvailable) {
            requestDevnetTokens(idx);
          } else {
            requestDevnetAirdrop(["USDC", "USDT", "DAI"][idx]);
          }
        });
      });

      if (provider) {
        provider.on("connect", (pubkey) => {
          setWalletPublicKey(pubkey || provider.publicKey);
          refreshWalletBalances({ silent: true });
        });
        provider.on("disconnect", () => {
          setWalletPublicKey(null);
        });
        provider.on("accountChanged", (pubkey) => {
          if (!pubkey) {
            setWalletPublicKey(null);
            return;
          }
          setWalletPublicKey(pubkey);
          refreshWalletBalances({ silent: true });
        });

        provider.connect({ onlyIfTrusted: true }).then((res) => {
          if (res?.publicKey || provider.publicKey) {
            setWalletPublicKey(res?.publicKey || provider.publicKey);
            refreshWalletBalances({ silent: true });
          }
        }).catch(() => {});
      }
    }

    async function fetchAll() {
      const [
        protocolRes,
        circuitRes,
        supplyRes,
        sigRes,
        agentsRes,
        vaultsResV1,
        vaultsResV2,
        usdcPythRes,
        usdtPythRes,
        daiPythRes
      ] = await Promise.all([
        rpc("getAccountInfo", [CFG.PROTOCOL_STATE, { encoding: "base64" }]),
        rpc("getAccountInfo", [CFG.CIRCUIT_BREAKER, { encoding: "base64" }]),
        rpc("getTokenSupply", [CFG.MSTB_MINT]),
        rpc("getSignaturesForAddress", [CFG.PROGRAM_ID, { limit: 10 }]),
        rpc("getProgramAccounts", [CFG.PROGRAM_ID, { encoding: "base64", filters: [{ dataSize: 168 }] }]),
        rpc("getProgramAccounts", [CFG.PROGRAM_ID, { encoding: "base64", filters: [{ dataSize: 200 }] }]),
        rpc("getProgramAccounts", [CFG.PROGRAM_ID, { encoding: "base64", filters: [{ dataSize: 264 }] }]),
        rpc("getAccountInfo", [CFG.FEEDS.USDC, { encoding: "base64" }]),
        rpc("getAccountInfo", [CFG.FEEDS.USDT, { encoding: "base64" }]),
        rpc("getAccountInfo", [CFG.FEEDS.DAI, { encoding: "base64" }])
      ]);

      const protocolBytes = base64ToBytes(protocolRes?.value?.data?.[0] || "");
      const circuitBytes = base64ToBytes(circuitRes?.value?.data?.[0] || "");

      const protocol = parseProtocolState(protocolBytes);
      const circuit = parseCircuitBreaker(circuitBytes);

      const supplyRaw = BigInt(supplyRes?.value?.amount || "0");

      const agents = (agentsRes || []).map((x) => {
        const b64 = x?.account?.data?.[0];
        if (!b64) return null;
        try {
          const bytes = base64ToBytes(b64);
          if (!hasDiscriminator(bytes, ACCOUNT_DISCRIMINATORS.AGENT_RECORD)) return null;
          return parseAgentRecord(bytes);
        } catch {
          return null;
        }
      }).filter(Boolean);

      const vaultEntries = [...(vaultsResV1 || []), ...(vaultsResV2 || [])];
      const vaultByIndex = new Map();
      for (const x of vaultEntries) {
        const b64 = x?.account?.data?.[0];
        if (!b64) continue;
        try {
          const parsed = parseCollateralVault(base64ToBytes(b64));
          if (!vaultByIndex.has(parsed.index)) vaultByIndex.set(parsed.index, parsed);
        } catch {}
      }
      const vaults = Array.from(vaultByIndex.values()).sort((a, b) => a.index - b.index);

      const signatures = sigRes || [];
      const typeMap = await enrichTxTypes(signatures);

      function parsePythEntry(res) {
        try {
          const b64 = res?.value?.data?.[0];
          if (!b64) return { error: "missing account data" };
          return parsePythPriceUpdate(base64ToBytes(b64));
        } catch (e) {
          return { error: e.message || "parse failure" };
        }
      }

      const oracles = {
        USDC: parsePythEntry(usdcPythRes),
        USDT: parsePythEntry(usdtPythRes),
        DAI: parsePythEntry(daiPythRes)
      };

      return { protocol, circuit, supplyRaw, agents, vaults, signatures, typeMap, oracles };
    }

    async function poll() {
      if (state.polling) return;
      state.polling = true;
      try {
        const data = await fetchAll();

        updateHealth(data.protocol, data.circuit, data.supplyRaw, data.vaults);
        updateOptimizer(data.protocol);
        updateAgents(data.agents, data.protocol);
        renderAgentRegistryPreview(data.agents);
        updateTxFeed(data.signatures, data.typeMap);
        updateOracles(data.oracles);

        state.lastVaults = data.vaults;
        state.lastAgents = data.agents;
        state.lastProtocol = data.protocol;
        state.collateralMints = resolveCollateralMints(data.protocol, data.vaults);
        updateMintEstimate();
        updateRedeemEstimate();
        updateMintButton();
        updateRedeemButton();
        refreshFaucetMintAuthorities({ silent: true });

        if (state.wallet.publicKey) {
          refreshWalletBalances({ silent: true });
        }

        const latestTx = data.signatures[0];
        if (latestTx?.blockTime) state.lastKeeperActivity = Number(latestTx.blockTime) * 1000;

        state.lastSuccessAt = Date.now();
        state.lastError = "";
        $("programShort").textContent = shortKey(CFG.PROGRAM_ID);

        setLive(true);
        updateHeaderTick();
      } catch (e) {
        state.lastError = e.message || String(e);
        setLive(false);
        $("rpcHealth").textContent = `RPC: ${shortKey(state.lastError)}`;
      } finally {
        state.polling = false;
      }
    }

    function boot() {
      $("programShort").textContent = shortKey(CFG.PROGRAM_ID);
      updateHeaderTick();
      drawHistoryChart();

      try {
        bindWalletAndActionUi();
      } catch (e) {
        setMintTxStatus("error", `Wallet init failed: ${shortKey(e.message || String(e))}`);
        setRedeemTxStatus("error", `Wallet init failed: ${shortKey(e.message || String(e))}`);
        setAgentRegisterStatus("error", `Wallet init failed: ${shortKey(e.message || String(e))}`);
      }

      poll();
      setInterval(poll, CFG.TICK_MS);
      setInterval(updateHeaderTick, 1000);

      window.addEventListener("resize", () => {
        clearTimeout(window.__chartResizeTimer);
        window.__chartResizeTimer = setTimeout(drawHistoryChart, 120);
      });
    }

    boot();