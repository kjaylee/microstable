# Microstable 미구현/Stub 전수 점검 (2026-02-22)

## 점검 범위
- `solana/programs/microstable/src/lib.rs` (2,652 lines)
- `solana/keeper/src/*.rs` 전 파일

## 점검 방법
1. 키워드 스캔: `TODO|STUB|unimplemented|placeholder|FIXME|mock|fake`
2. 수동 코드 리뷰: 빈/우회 로직, 하드코딩된 가짜 값, 운영 경로에서 실패를 유발하는 상수
3. devnet 실행 검증: `anchor deploy`, `keeper --once`

> 키워드 기반 TODO/STUB는 직접적으로 발견되지 않았으나, 운영상 미구현/가짜값 성격의 항목이 아래와 같이 확인됨.

## 발견 결과

| 파일:라인 | 미구현/문제 내용 | 심각도 | 수정 필요 여부 |
|---|---|---|---|
| `solana/programs/microstable/src/lib.rs:466, 2149-2194` | Pyth `write_authority` 검증이 하드코딩 allowlist 중심이라 devnet 실계정(`write_authority == price_account`)을 거부. Keeper oracle update 실운영 실패 유발. | **CRITICAL** | **YES (수정 완료)** |
| `solana/keeper/src/oracle.rs:17, 474-482` | Keeper도 동일하게 `PYTH_TRUSTED_WRITE_AUTHORITY` 단일 신뢰값 기반 검증. 실제 devnet feed 업데이트를 거부. | **CRITICAL** | **YES (수정 완료)** |
| `solana/keeper/src/config.rs:134` | 기본 secondary RPC가 `https://secondary-rpc.devnet.example.invalid`로 하드코딩(placeholder). 정상 dual-RPC 검증 불가. | **HIGH** | **YES (수정 완료)** |
| `solana/programs/microstable/src/lib.rs:516` | `mint`가 주석상 “simulation-only ledger update”; 실제 MSTB SPL mint 발행 경로 미구현(장부 수치만 증가). | **HIGH** | **YES (미완, 아키텍처 변경 필요)** |
| `solana/programs/microstable/src/lib.rs:746` | `redeem`가 주석상 “simulation-only ledger burn”; 실제 MSTB SPL burn 경로 미구현(장부 수치만 감소). | **HIGH** | **YES (미완, 아키텍처 변경 필요)** |
| `solana/programs/microstable/src/lib.rs:32` | `PYTH_USDS_USD` 고정 계정(`9h4r...`)이 devnet `AccountNotFound` 확인됨. index=3 feed 설정/업데이트 경로 잠재 실패. | **MEDIUM** | **YES (후속 조치 권장)** |
| `solana/keeper/src/utils.rs:1158` | `resolve_expected_binary_sha256`가 “Legacy helper retained” 상태. 테스트 호환용 잔존 코드. | **LOW** | NO (의도된 호환 코드) |

## 이번 라운드 실제 수정 반영
- ✅ `lib.rs`: Pyth write_authority 허용 규칙을 **allowlist OR price_account self-authority**로 보완
- ✅ `keeper/src/oracle.rs`: 동일 규칙으로 보완
- ✅ `keeper/src/config.rs`: 기본 secondary RPC를 실사용 가능한 `https://devnet.rpcpool.com`으로 변경
- ✅ `keeper/config.devnet.json`도 동일 endpoint로 갱신

## 잔여 미구현(중요)
- `mint/redeem`의 “simulation-only” 상태(실제 MSTB SPL mint/burn 미구현)는 구조 변경(계정 모델 + CPI 경로 + 권한 설계) 필요.
