# Purple Team Report — Continuous Vulnerability Discovery

## Summary
- 총 발견: **27개**
- **CRITICAL: 2개, HIGH: 14개, MEDIUM: 11개, LOW: 0개**
- 분석 기준 파일:
  - `microstable.py`
  - `open_agent_economy.py`
  - `adversarial_agents.py`
  - `solana/programs/microstable/src/lib.rs`
  - `docs/open-agent-economy.md`
  - `security/red_team_exploits.py` (요청된 `tests/red_team_v1.py` 대체)
  - `security/red_team_v2_exploits.py` (요청된 `tests/red_team_v2.py` 대체)

## Findings

### PT-001: `claim_reward` 무제한 보상 발행
- **등급:** 🔴 CRITICAL
- **카테고리:** 경제
- **라인 참조:** `open_agent_economy.py:417-422`
- **설명:** `claim_id` 중복만 막고 보상 출처/한도/증빙 검증이 없다. 임의 `claim_id`를 계속 바꿔 호출하면 무한 보상 발행이 가능하다.
- **공격 시나리오:**
  1. 공격자가 유효한 에이전트 ID를 하나 확보한다.
  2. `claim_reward(agent_id, huge_amount, unique_claim_id, epoch)`를 반복 호출한다.
  3. `balances`가 외부 수익 없이 계속 증가한다.
- **영향:** 스테이킹 경제 붕괴, 보상 인플레이션, 트레저리/토큰 신뢰 훼손.
- **Exploit 가능성:** 매우 높음. PoC에서 2회 호출만으로 잔고 `10 -> 3,000,010` 확인.
- **기존 방어:** claim_id 중복 체크만 존재.
- **권장 대응:** claim에 서명된 지급근거/예산 한도/epoch 캡을 필수화.

### PT-002: 슬래시 이후 출금 오버드로우
- **등급:** 🟠 HIGH
- **카테고리:** 경제
- **라인 참조:** `open_agent_economy.py:385-399, 401-410`
- **설명:** 출금 요청 시점에만 잔고를 확인하고 잠그지 않는다. 쿨다운 중 슬래시되어도 `withdraw`는 예약된 원금 전액을 반환한다.
- **공격 시나리오:**
  1. 100 스테이크로 출금 예약(`pending=100`)을 건다.
  2. 그 사이 슬래시로 실제 잔고를 10으로 줄인다.
  3. 쿨다운 후 `withdraw()`가 100을 반환한다.
- **영향:** 페널티 회피, 경제적 무결성 손상, 회계 불일치.
- **Exploit 가능성:** 높음. PoC에서 `slash 후 balance=10`, `withdraw 반환=100` 재현.
- **기존 방어:** 없음(출금 예약 금액 락 없음).
- **권장 대응:** 출금 요청 즉시 해당 금액을 락하고, 출금 시 재검증.

### PT-003: Reveal 재호출로 동일 제안 중복 반영
- **등급:** 🟠 HIGH
- **카테고리:** 프로토콜
- **라인 참조:** `open_agent_economy.py:567-582`
- **설명:** 동일 commit에 대해 `reveal()`을 여러 번 호출해도 차단하지 않는다. 제안 리스트에 동일 제안이 중복 적재된다.
- **공격 시나리오:**
  1. 1회 commit 후 reveal 윈도우 진입.
  2. 동일 proposal/secret으로 `reveal()` 반복 호출.
  3. `proposals`에 같은 proposal이 다수 쌓인다.
- **영향:** 평가/보상 왜곡, 참가자 풀 보상 편취.
- **Exploit 가능성:** 높음. PoC에서 `reveal1=True, reveal2=True, proposal_count=2` 확인.
- **기존 방어:** commit hash 일치만 확인, one-time consume 없음.
- **권장 대응:** reveal 성공 시 commit을 소모 처리하고 agent당 1회 제한.

### PT-004: `submit_direct`로 Commit-Reveal 우회
- **등급:** 🟠 HIGH
- **카테고리:** 프로토콜
- **라인 참조:** `open_agent_economy.py:584-594`
- **설명:** `submit_direct`는 commit/reveal 경로를 통째로 건너뛴다. anti-gaming 설계가 비활성화된다.
- **공격 시나리오:**
  1. 공격자는 공개 상태를 본 뒤 마지막에 직접 제출한다.
  2. commit 비밀/타이밍 비용 없이 유리한 파라미터를 넣는다.
  3. 평가에 정상 참여한다.
- **영향:** MEV/카피캣 저항 구조 붕괴.
- **Exploit 가능성:** 높음. PoC에서 commit 없이 `direct=True` 재현.
- **기존 방어:** stake 체크만 있음.
- **권장 대응:** 운영 모드에서 direct 경로 비활성화 또는 별도 신뢰 경로로 제한.

### PT-005: Score 함수 자기신고값 조작(리스크 0)
- **등급:** 🟠 HIGH
- **카테고리:** 경제
- **라인 참조:** `open_agent_economy.py:596-609`
- **설명:** `expected_return / risk`를 외부 검증 없이 그대로 점수화한다. `risk=0` 근처로 제출하면 점수가 사실상 무한대로 튄다.
- **공격 시나리오:**
  1. 공격자가 실제로는 위험한 제안을 만든다.
  2. 제출 payload에서 `risk=0`, `expected_return`을 크게 설정한다.
  3. 평가에서 압도적 1위를 차지한다.
- **영향:** 악성 제안 채택, 파라미터 드리프트, 손실 확대.
- **Exploit 가능성:** 높음. PoC 점수 비교: honest `-0.17`, attacker `99,999,999.77`.
- **기존 방어:** 없음(자가보고 값 검증 부재).
- **권장 대응:** 리스크/리턴을 독립 엔진으로 재계산.

### PT-006: 우승 제안 파라미터 무검증 적용
- **등급:** 🔴 CRITICAL
- **카테고리:** 프로토콜
- **라인 참조:** `open_agent_economy.py:611-623`
- **설명:** 승자 파라미터를 `current_params`에 그대로 반영하며 weight sum, 범위, fee 상한 검증이 없다.
- **공격 시나리오:**
  1. 공격자가 비정상 weights/fee를 가진 proposal 제출.
  2. 점수 우위를 만들면 우승.
  3. 시스템 파라미터에 그대로 반영된다.
- **영향:** 시스템 상태 비정상화, 후속 로직 오동작/경제 붕괴.
- **Exploit 가능성:** 높음. PoC에서 `{weights:[5,-1,0,0], mint_fee:0.5}`가 그대로 적용됨.
- **기존 방어:** 없음.
- **권장 대응:** 채택 직전 하드 인바리언트 검증(합/범위/캡/fee).

### PT-007: 참가자 풀 보상 시빌링(중복 proposal 스팸)
- **등급:** 🟠 HIGH
- **카테고리:** 경제
- **라인 참조:** `open_agent_economy.py:634-637`
- **설명:** participant_pool을 “고유 에이전트”가 아니라 “proposal 개수”로 분배한다. 동일 에이전트가 중복 proposal을 쌓아 풀 보상을 빨아간다.
- **공격 시나리오:**
  1. 같은 agent_id로 proposal 여러 건을 삽입한다(PT-003/submit_direct 결합).
  2. 평가 시 participant_pool이 proposal 수 기준으로 쪼개진다.
  3. 공격자가 대부분 조각을 회수한다.
- **영향:** 보상 왜곡, 정직 참가자 이탈.
- **Exploit 가능성:** 높음. PoC에서 공격자 44.16 vs 타 참가자 0.83.
- **기존 방어:** 없음.
- **권장 대응:** epoch당 agent당 1개 유효 proposal만 인정.

### PT-008: Watchdog `resolve` 재실행으로 중복 보상/중복 슬래시
- **등급:** 🟠 HIGH
- **카테고리:** 에이전트
- **라인 참조:** `open_agent_economy.py:696-711`
- **설명:** resolve 후 상태를 소모/종결하지 않는다. 같은 `(epoch, alert_type)`에 대해 재호출 시 보상/패널티가 반복 적용된다.
- **공격 시나리오:**
  1. 유효 report 1건 생성.
  2. `resolve(..., is_true=True)`를 여러 번 호출.
  3. 동일 monitor가 반복 보상을 받는다(또는 false로 반복 슬래시).
- **영향:** 보상 풀 고갈/악의적 처벌.
- **Exploit 가능성:** 높음. PoC에서 2회 resolve로 balance `2.0`, rep `40`.
- **기존 방어:** 없음.
- **권장 대응:** alert별 one-shot finalize 상태를 도입.

### PT-009: 미래 timestamp 증거 허용(신선도 검증 우회)
- **등급:** 🟡 MEDIUM
- **카테고리:** 에이전트
- **라인 참조:** `open_agent_economy.py:671-678`
- **설명:** `epoch - evidence.timestamp > max_age`만 검사하여, 미래 timestamp는 음수가 되어 항상 통과한다.
- **공격 시나리오:**
  1. 공격자가 미래 시점 timestamp를 넣은 증거 제출.
  2. stale 거부 로직을 우회.
  3. 과거/조작 증거를 장기간 재사용.
- **영향:** 경보 신뢰도 저하, 오탐/악용 증가.
- **Exploit 가능성:** 높음. PoC `timestamp=10^9`에서도 `report_future=True`.
- **기존 방어:** 과거 stale만 차단.
- **권장 대응:** `timestamp <= epoch` 및 허용 드리프트 범위를 함께 검증.

### PT-010: 사전순 ID 선점으로 바운티 스나이핑
- **등급:** 🟡 MEDIUM
- **카테고리:** 경제
- **라인 참조:** `open_agent_economy.py:701-704`
- **설명:** “첫 신고자”를 실제 도착순이 아니라 문자열 정렬 최소값으로 선정한다.
- **공격 시나리오:**
  1. 공격자가 `aaa...` 형태 ID로 등록.
  2. 타 모니터와 동일 타이밍 신고.
  3. resolve 시 정렬 우선권으로 바운티 독식.
- **영향:** 인센티브 왜곡, 신고 품질 저하.
- **Exploit 가능성:** 높음. PoC에서 `aaa`만 보상 획득.
- **기존 방어:** 없음.
- **권장 대응:** 서명된 도착 시각/시퀀스 기반 first-reporter 판정.

### PT-011: ACP 메시지 재전송(Replay) 상시 유효
- **등급:** 🟠 HIGH
- **카테고리:** 에이전트
- **라인 참조:** `open_agent_economy.py:452-468`
- **설명:** 서명 검증에 nonce/만료/epoch 바인딩이 없다. 동일 패킷을 재전송해도 검증이 계속 통과한다.
- **공격 시나리오:**
  1. 유효 ACP 메시지 1개를 캡처.
  2. 동일 payload를 반복 전송.
  3. 수신측이 별도 중복방지 없으면 반복 실행.
- **영향:** 중복 실행, 보상/투표/제안 재적용.
- **Exploit 가능성:** 높음. PoC에서 동일 메시지 `verify1=True`, `verify2=True`.
- **기존 방어:** payload 해시 검증만 존재.
- **권장 대응:** nonce/expiry/epoch/state-hash를 서명 본문에 포함.

### PT-012: ACP 공유 비밀키 모델로 에이전트 사칭
- **등급:** 🟠 HIGH
- **카테고리:** 에이전트
- **라인 참조:** `open_agent_economy.py:452-468`
- **설명:** 검증 키가 agent별 공개키가 아니라 호출자가 넘기는 `secret`이다. 공유 비밀 유출 시 타 agent_id를 손쉽게 사칭 가능하다.
- **공격 시나리오:**
  1. 공용 secret 하나를 탈취.
  2. 피해자 `agent_id`를 payload에 넣어 새 메시지 서명 생성.
  3. verify 통과 후 피해자 행위로 처리.
- **영향:** 신원 무결성 붕괴.
- **Exploit 가능성:** 높음. PoC forged message `forged_valid=True`.
- **기존 방어:** 없음(Ed25519 per-agent 미구현).
- **권장 대응:** agent registry pubkey 기반 비대칭 서명 검증으로 전환.

### PT-013: Epoch 파라미터 스푸핑으로 RateLimit 우회
- **등급:** 🟡 MEDIUM
- **카테고리:** 프로토콜
- **라인 참조:** `open_agent_economy.py:471-482`
- **설명:** 제한 키가 `(agent_id, epoch)`인데 epoch는 호출자가 임의 입력한다. epoch를 계속 바꿔 무제한 요청이 가능하다.
- **공격 시나리오:**
  1. 같은 agent_id로 요청을 보낸다.
  2. 매 요청 epoch를 증가시킨다.
  3. per-epoch 한도를 사실상 무효화한다.
- **영향:** 메시지 스팸/DoS, ACP 처리 지연.
- **Exploit 가능성:** 높음. PoC `[True, True, False, True, True]`.
- **기존 방어:** 없음.
- **권장 대응:** epoch를 신뢰원(clock/state)에서만 파생.

### PT-014: RedemptionQueue 평균 할인율 오염(Discount Poisoning)
- **등급:** 🟡 MEDIUM
- **카테고리:** 경제
- **라인 참조:** `microstable.py:1545-1553`
- **설명:** 배치의 `requested_discount_ppm`을 burn 가중 없이 단순 평균한다. 소량 공격 요청 하나로 대형 정상 요청의 할인율을 크게 깎을 수 있다.
- **공격 시나리오:**
  1. 피해자 대량 redeem(1,000,000 ppm) 대기.
  2. 공격자가 극단값(0 ppm) 소량 요청 삽입.
  3. 배치 평균 할인율이 하락해 피해자 지급액 감소.
- **영향:** 선량 사용자 가치 손실, 배치 공정성 붕괴.
- **Exploit 가능성:** 높음. PoC에서 피해자가 즉시 저지급됨.
- **기존 방어:** 없음.
- **권장 대응:** burn 가중 평균 또는 요청별 개별 할인 정산.

### PT-015: RedemptionQueue 마지막 순번 잔여분 편취
- **등급:** 🟡 MEDIUM
- **카테고리:** 경제
- **라인 참조:** `microstable.py:1558-1564`
- **설명:** 소수점 절삭 후 잔여는 배치 마지막 계정이 전부 받는다. 공격자가 last position을 잡으면 반복적으로 rounding dust를 회수 가능하다.
- **공격 시나리오:**
  1. 공격자가 여러 계정으로 큐 순서를 조작해 마지막에 위치.
  2. 배치 정산 시 앞선 사용자들은 floor 분배.
  3. 마지막 계정이 residual을 독식.
- **영향:** 장기 누적 가치 유출.
- **Exploit 가능성:** 중간. PoC에서 마지막 계정이 불균형 잔여를 수취.
- **기존 방어:** 없음.
- **권장 대응:** 잔여분을 라운드로빈/무작위/비례 분배.

### PT-016: CB4 롤백 경로에서 CB3 민팅 제한 완화 불일치
- **등급:** 🟠 HIGH
- **카테고리:** 프로토콜
- **라인 참조:** `microstable.py:1136-1143`, `microstable.py:1822-1827`
- **설명:** 일반 CB3 활성 시 `mint_limit=0.0`인데, CB4 롤백 분기에서는 `mint_limit=min(...,0.10)`으로 부분 허용된다. 동일 위험상태에서 정책 불일치가 발생한다.
- **공격 시나리오:**
  1. CB3(oracle degraded) + CB4(rollback) 동시 유도.
  2. 롤백 브랜치 진입 후 mint_limit이 10%로 남음.
  3. 위험 상태에서 제한적 민팅을 지속 시도.
- **영향:** 오라클 저하 상태 민팅 노출, 위험 전파.
- **Exploit 가능성:** 중간~높음(조건 조합 필요).
- **기존 방어:** 부분(일반 경로만 0.0).
- **권장 대응:** CB3 정책을 모든 분기에서 단일 규칙으로 강제.

### PT-017: `commit_rebalance` 덮어쓰기 Griefing
- **등급:** 🟠 HIGH
- **카테고리:** 프로토콜
- **라인 참조:** `lib.rs:532-555`
- **설명:** pending commit 존재 여부 확인 없이 새 commit이 항상 덮어쓴다. 마지막 순간 덮어쓰기로 정상 reveal을 무효화할 수 있다.
- **공격 시나리오:**
  1. 정상 운영자가 commit 등록.
  2. 공격 측 키 쿼럼이 동일 슬롯대에 다른 commit으로 overwrite.
  3. 원래 reveal은 mismatch로 실패.
- **영향:** 리밸런스 지연/무력화, 운영 DoS.
- **Exploit 가능성:** 높음(keeper 쿼럼 내부자/탈취 시).
- **기존 방어:** 없음(단순 대입).
- **권장 대응:** pending commit 존재 시 overwrite 금지 또는 버전 nonce 사용.

### PT-018: 소규모 분할 리밸런스로 Commit-Reveal 보호 우회
- **등급:** 🟠 HIGH
- **카테고리:** 복합
- **라인 참조:** `lib.rs:588-600`
- **설명:** commit/reveal 요구는 `turnover >= 5%`일 때만 발동한다. 공격자는 2% step 제한 내 다중 트랜잭션으로 총 6~10% 이동을 누적해 보호를 우회할 수 있다.
- **공격 시나리오:**
  1. 한 번에 5% 미만(예: 2%~4%) 리밸런스 tx를 연속 제출.
  2. 각 tx는 commit/reveal 없이 통과.
  3. 총 변화량은 대규모이나 공개 보호 없이 완료.
- **영향:** 예측가능 주문 노출, 우회된 MEV 방어.
- **Exploit 가능성:** 높음.
- **기존 방어:** 대규모 단건만 보호.
- **권장 대응:** 누적 turnover 기준으로도 commit/reveal 트리거.

### PT-019: `risk_score` 사문화로 독성 담보 1:1 평가
- **등급:** 🟠 HIGH
- **카테고리:** 경제
- **라인 참조:** `lib.rs:1140`, `lib.rs:1445-1457`
- **설명:** vault에 `risk_score` 필드가 있으나 가치 계산(`total_collateral_value`)에 반영되지 않는다. 고위험 담보도 동일 가격 곱셈으로 CR을 채운다.
- **공격 시나리오:**
  1. 고위험 자산 vault 비중을 늘린다.
  2. 오라클 가격이 유지되는 동안 동일 가치로 인정받는다.
  3. 충격 시 실제 손실이 CR 계산보다 크게 발생.
- **영향:** CR 과대평가, 잠복 부실 축적.
- **Exploit 가능성:** 높음(느린 공격).
- **기존 방어:** 필드만 존재, 계산 미반영.
- **권장 대응:** valuation 단계에서 risk haircut을 강제 적용.

### PT-020: 단일 담보 오라클 저하로 전체 mint 중단 DoS
- **등급:** 🟠 HIGH
- **카테고리:** 인프라
- **라인 참조:** `lib.rs:212-214`, `lib.rs:1460-1465`
- **설명:** mint는 모든 vault 중 하나라도 degraded면 전면 중지된다(`any`). 공격자는 한 자산 피드만 stale/confidence 초과로 만들어도 전체 민팅을 멈출 수 있다.
- **공격 시나리오:**
  1. 1개 담보 오라클 업데이트를 지연/검열.
  2. 해당 vault만 degraded 상태 유발.
  3. 모든 collateral mint 경로가 `OracleDegraded`로 실패.
- **영향:** 전역 민팅 liveness 상실.
- **Exploit 가능성:** 중간~높음(운영/피드 교란 필요).
- **기존 방어:** 없음(per-vault 격리 미구현).
- **권장 대응:** 자산별 격리 정책으로 degraded 범위를 국소화.

### PT-021: Attack ID 그라인딩으로 성공·미탐지 조합 사전 탐색
- **등급:** 🟠 HIGH
- **카테고리:** 에이전트
- **라인 참조:** `adversarial_agents.py:393-398`
- **설명:** 성공/탐지 난수가 `attack_id`와 epoch에 결정론적으로 매핑된다. 공격자는 ID를 브루트포스해 성공+미탐지 케이스를 선별 가능하다.
- **공격 시나리오:**
  1. 고정된 방어강도/epoch에서 후보 attack_id를 대량 생성.
  2. 로컬에서 success/detected 결과를 미리 계산.
  3. 유리한 ID만 실제 제출.
- **영향:** 탐지 회피율 상승, 방어 학습 왜곡.
- **Exploit 가능성:** 높음. PoC에서 5만 탐색 내 `id18`에서 성공+미탐지 발견.
- **기존 방어:** 없음.
- **권장 대응:** 비밀 난수/서버 nonce를 결과 샘플링에 결합.

### PT-022: 시그니처 버킷 충돌로 차단목록 오염/우회
- **등급:** 🟡 MEDIUM
- **카테고리:** 에이전트
- **라인 참조:** `adversarial_agents.py:342-350`
- **설명:** 시그니처가 intensity 3자리 반올림 + 16 hex 축약이다. 서로 다른 공격이 동일 signature로 매핑되어 과차단/우회가 가능하다.
- **공격 시나리오:**
  1. intensity를 미세 조정해 동일 반올림 버킷에 맞춘다.
  2. benign/critical 공격을 같은 signature로 충돌시킨다.
  3. 블록리스트를 오염시키거나 변형 공격을 우회한다.
- **영향:** 탐지 정확도 저하, 운영 혼란.
- **Exploit 가능성:** 높음. PoC에서 intensity 0.12341 vs 0.12349가 동일 signature.
- **기존 방어:** 없음.
- **권장 대응:** 전체 파라미터+충분한 길이의 충돌저항 서명 사용.

### PT-023: Collusion 탐지 스키마 불일치 블라인드스팟
- **등급:** 🟠 HIGH
- **카테고리:** 에이전트
- **라인 참조:** `adversarial_agents.py:546-557`
- **설명:** 탐지기는 proposal에서 `vector` 키를 읽는데, 실제 OAE proposal은 `weights`를 사용한다. 통합 시 유사 제안도 탐지되지 않는다.
- **공격 시나리오:**
  1. 공모 에이전트가 동일 weights 제출.
  2. 탐지기는 `vector` 미존재로 0 유사도로 처리.
  3. collusion 경보 없이 통과.
- **영향:** 공모 제안 대량 통과, 토너먼트 조작.
- **Exploit 가능성:** 높음. PoC에서 weights 기반 입력은 `[]`, vector 키로만 탐지됨.
- **기존 방어:** 없음(데이터 계약 불일치).
- **권장 대응:** 공통 스키마 강제 및 입력 정규화 계층 도입.

### PT-024: Alert ID 랜덤화로 대응 idempotency 무력화
- **등급:** 🟡 MEDIUM
- **카테고리:** 운영
- **라인 참조:** `adversarial_agents.py:682-688`
- **설명:** 중복 방지는 `alert_id` 문자열 기준이다. 동일 사건이라도 id만 바꾸면 매번 새 대응이 발동한다.
- **공격 시나리오:**
  1. 동일 이벤트를 다른 id로 반복 제출.
  2. `handled_alerts` 중복체크를 우회.
  3. rate_limit/freeze/quarantine를 과도하게 반복 유발.
- **영향:** 자동대응 과민반응, 운영 DoS.
- **Exploit 가능성:** 높음. PoC에서 id만 바꿔 연속 `rate_limit` 발동.
- **기존 방어:** 없음(내용 기반 중복판정 부재).
- **권장 대응:** alert 본문 정규화 해시 기반 idempotency 적용.

### PT-025: SAFE_MODE 자동 해제의 건강도 검증 부재
- **등급:** 🟡 MEDIUM
- **카테고리:** 운영
- **라인 참조:** `adversarial_agents.py:743-749`
- **설명:** 5 epoch 경과만으로 safe_mode를 자동 해제한다. 위협이 지속돼도 상태가 풀릴 수 있다.
- **공격 시나리오:**
  1. 공격자가 저강도 공격을 지속해 위험 상태 유지.
  2. 시스템은 시간 기준만으로 safe_mode 해제.
  3. 방어 장치가 조기 해제된 구간에 재공격.
- **영향:** 방어 공백 창 발생.
- **Exploit 가능성:** 중간. PoC에서 조건 없이 safe_mode=False 전환 확인.
- **기존 방어:** 없음(헬스체크 gate 없음).
- **권장 대응:** 해제 전 위험지표/이상탐지 상태를 필수 조건화.

### PT-026: 포렌식 서명 길이/도메인 불일치로 차단 실패
- **등급:** 🟡 MEDIUM
- **카테고리:** 에이전트
- **라인 참조:** `adversarial_agents.py:342-350`, `adversarial_agents.py:787-801`
- **설명:** 실행기 차단 서명은 16 hex, 포렌식 서명은 20 hex라 네임스페이스가 다르다. 포렌식에서 생성한 서명을 그대로 차단목록에 넣어도 실행 차단이 안 된다.
- **공격 시나리오:**
  1. 포렌식이 악성 패턴 서명 생성.
  2. 운영자가 해당 서명을 executor 차단목록에 추가.
  3. 동일 공격이 여전히 실행됨(서명 포맷 미스매치).
- **영향:** 대응 자동화 실패.
- **Exploit 가능성:** 높음. PoC에서 forensic sig 등록 후에도 `status=failed`(blocked 아님) 확인.
- **기존 방어:** run_round 내 일부 경로에서만 exact_sig를 별도 추가.
- **권장 대응:** 단일 canonical signature 스킴으로 통합.

### PT-027: 포렌식 시그니처 피처 축약으로 손쉬운 변형 회피
- **등급:** 🟡 MEDIUM
- **카테고리:** 에이전트
- **라인 참조:** `adversarial_agents.py:787-797`
- **설명:** signature material이 `vector/tier/timing/scale_bucket/chain_depth`만 포함한다. intensity·stealth·budget 변화는 같은 버킷이면 동일 서명으로 취급된다.
- **공격 시나리오:**
  1. 차단된 공격과 동일 버킷 특성 유지.
  2. intensity/stealth만 바꿔 실질 페이로드를 변형.
  3. 동일 또는 인접 서명 규칙을 이용해 탐지/차단 회피.
- **영향:** 블랙리스트 우회, 변형 공격 지속.
- **Exploit 가능성:** 중간. PoC에서 서로 다른 공격 레코드가 동일 포렌식 서명 생성.
- **기존 방어:** 없음.
- **권장 대응:** 공격 의미를 보존하는 고차원 피처를 서명 재료에 포함.
