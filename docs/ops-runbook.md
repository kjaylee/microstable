# Microstable Operations Runbook

> Version: 2026-02-22 (Green Team)
> Scope: 운영 준비, 모니터링, 장애 대응, Devnet→Mainnet 이행 절차

---

## 1) 일상 운영 절차 (Daily Operations)

### 1.1 배포 전 체크
1. 코드 동결 확인 (`main` 브랜치 보호 + 리뷰 완료)
2. `python3 -m py_compile monitoring/*.py chaos/*.py`
3. `python3 chaos/runner.py` 실행 후 `outputs/chaos/chaos-results.json` PASS 확인
4. `python3 chaos/degradation_test.py` 실행 후 `outputs/chaos/degradation-test-results.json` PASS 확인
5. `monitoring/healthcheck.py` 상태 확인 (`healthy|degraded|critical`)

### 1.2 배포 절차 (오프체인/온체인)
- 오프체인 에이전트:
  - Keeper / Watchdog / Auditor 프로세스 기동
  - 초기 dry-run probe (응답 시간/JSON 정상 여부 확인)
- 온체인(Solana):
  - `solana config get`로 cluster 재확인
  - `anchor deploy` (환경별 승인 정책 적용)
  - 초기 트랜잭션 smoke test (mint/redeem/read-only)

### 1.3 운영 모니터링
- 핵심 대시보드 데이터:
  - `docs/dashboard-data.json`
  - `outputs/chaos/dashboard-data.json`
- 필수 KPI:
  - Peg MAE / max error
  - CR(current/min)
  - CB activation count + 연속 활성화 길이
  - Agent health (last activity, response latency)
  - Throughput / failure rate
  - Oracle stale streak
  - Memory/CPU

### 1.4 백업 절차
- 최소 백업 대상:
  - `.state/agents/security_state.json`
  - `outputs/chaos/*`
  - `outputs/monitoring/*`
  - `docs/dashboard-data.json`
- 백업 주기:
  - 상태 파일: 5분
  - 리포트/로그: 1시간
  - 배포 직전/직후: 수동 스냅샷 강제

---

## 2) 장애 대응 매뉴얼 (Incident Playbook)

### 2.1 심각도 정의
- **CRITICAL**: Peg deviation >2%, CR<110%, CB 연속 활성화>3, 자금보전 불변식 위반
- **WARNING**: Agent 무응답>5 ticks, Oracle stale>10 ticks, Memory>1GB

### 2.2 공통 대응 단계
1. 감지: alert 발생 + healthcheck 상태 수집
2. 분류: oracle/agent/network/memory/config/race
3. 격리: 신규 민팅 제한(`mint_limit=0`) 및 보수 모드
4. 복구: 원인 수정 후 단계적 해제
5. 검증: degradation test + invariant 재검증

### 2.3 유형별 즉시 조치
- **Oracle 장애**: CB-3 강제 유지, optimizer 비활성화, oracle feed 복구 전 mint 중단
- **Agent 장애**: 생존 quorum 확인(2-of-3), 미충족 시 SAFE_MODE/FROZEN 적용
- **네트워크 분할**: keeper 제안 rate-limit, watchdog cooldown 유지, 잘못된 빠른 재시도 차단
- **메모리 압력**: autograd depth cap 확인, 프로세스 재기동, 과도한 trace 수집 중지
- **설정 폭주**: 비정상 proposal reject 비율 점검(>=90% 유지)

---

## 3) Agent 재시작/교체 절차

### 3.1 재시작
1. 현재 상태 백업 (`.state/agents/security_state.json`)
2. 해당 agent 프로세스 graceful stop
3. `--dry-run`으로 정상 JSON 응답 확인
4. `--execute` 재합류
5. agent health 지표(응답 시간/마지막 활동 tick) 정상화 확인

### 3.2 교체
1. 신규 인스턴스 준비 + 시크릿 주입
2. 기존 인스턴스와 overlap 운영(짧은 병행)
3. 2-of-3 quorum 안정성 확인
4. 기존 인스턴스 제거

---

## 4) Emergency Shutdown 절차

1. 조건: CRITICAL + 자금보전 위험 징후
2. 즉시 조치:
   - mint 전면 중지 (`mint_limit=0`)
   - 오라클 이상 시 CB-3 유지
   - 가능 시 redeem-only 모드로 전환
3. 커뮤니케이션:
   - 장애 공지, 영향 범위, 예상 복구시간
4. 복구 승인 기준:
   - healthcheck `healthy` 또는 제한적 `degraded`
   - degradation test PASS
   - funds preservation 검증 PASS

---

## 5) Devnet → Mainnet 마이그레이션 체크리스트

- [ ] Devnet/localnet chaos 8개 시나리오 PASS
- [ ] degradation test PASS
- [ ] alert 룰 검증 완료
- [ ] 운영자 온콜 체계/에스컬레이션 문서화
- [ ] 백업/복구 drill 완료
- [ ] RPC 다중 엔드포인트 failover 확인
- [ ] key rotation rehearsal 완료
- [ ] mainnet 배포 윈도우/롤백 계획 승인

---

## 6) 키 관리/로테이션 절차

1. 키 저장: HSM 또는 최소 권한 비밀 저장소
2. 분리: keeper/watchdog/auditor 키 분리 보관
3. 로테이션 주기: 30~90일, 사고 시 즉시
4. 로테이션 단계:
   - 신규 키 생성
   - 병행 검증(dry-run 서명)
   - 트래픽 절반 전환
   - 완전 전환 후 구 키 폐기
5. 감사: 로테이션 로그, 승인자, 시간 기록 필수

---

## 7) 운영 완료 조건 (Go/No-Go)
- Chaos/Degradation 전 항목 PASS
- Healthcheck가 최소 `degraded` 이상, critical fail 0
- 자금보전 불변식 위반 0
- 운영팀이 rollback 절차를 실제로 재현 가능
