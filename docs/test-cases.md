# microstable 테스트케이스 명세 (TDD)

- 문서 버전: v0.1
- 대상 구현: `microstable.py` (Phase 1)
- 목적: 구현 이전 테스트케이스 고정 (테스트 통과 시에만 구현 완료 인정)
- 총 TC 수: **50개**
  - A. Value autograd: 12개
  - B. Loss 함수: 8개
  - C. Optimizer: 8개
  - D. Circuit Breaker: 10개
  - E. 시나리오 통합: 8개
  - F. Agent 인터페이스: 4개

---

## 공통 테스트 전제

- 수치 비교 허용 오차: `atol=1e-9`, `rtol=1e-6` (명시 없는 경우)
- 수치 안정성 기준: 모든 산출값/gradient가 `NaN`/`Inf`가 아니어야 함
- 기본 하이퍼파라미터(스펙 기준)
  - `lambda_p=5.0`, `lambda_cr=20.0`, `lambda_var=2.0`, `lambda_turn=0.5`, `lambda_conc=1.5`, `lambda_orc=3.0`
  - Adam: `lr=0.005`, `beta1=0.9`, `beta2=0.999`, `eps=1e-8`
  - `grad_clip_norm=1.0`
  - 파라미터 변화 제한: `|Δw_i| <= 0.02`, `|Δfee| <= 0.001(10bps)`
- 바스킷 기본 자산 순서: `[USDC, USDT, DAI, USDS]`
- 기본 상한: `[0.55, 0.45, 0.45, 0.35]`
- 기본 초기 가중치: `[0.40, 0.30, 0.20, 0.10]`

---

## A. Value autograd (12)

### TC-V001
- ID: TC-V001
- 카테고리: Value autograd
- 설명: Value 덧셈 forward 정확성
- 입력:
  - `a=Value(1.25)`, `b=Value(-0.75)`
  - `y = a + b`
- 기대출력:
  - `y.data == 0.5`
- 판정기준(PASS/FAIL):
  - PASS: `abs(y.data - 0.5) <= atol`
  - FAIL: 위 조건 불만족

### TC-V002
- ID: TC-V002
- 카테고리: Value autograd
- 설명: Value 덧셈 backward gradient 정확성
- 입력:
  - `a=Value(2.0)`, `b=Value(3.0)`
  - `y = a + b`, `y.backward()`
- 기대출력:
  - `a.grad == 1.0`, `b.grad == 1.0`
- 판정기준(PASS/FAIL):
  - PASS: 두 gradient가 모두 1.0(허용오차 내)
  - FAIL: 하나라도 다름

### TC-V003
- ID: TC-V003
- 카테고리: Value autograd
- 설명: 곱셈 forward/backward 정확성
- 입력:
  - `a=Value(2.0)`, `b=Value(-3.0)`
  - `y=a*b`, `y.backward()`
- 기대출력:
  - `y.data == -6.0`
  - `a.grad == -3.0`, `b.grad == 2.0`
- 판정기준(PASS/FAIL):
  - PASS: forward/backward 모두 기대값 일치
  - FAIL: 하나라도 불일치

### TC-V004
- ID: TC-V004
- 카테고리: Value autograd
- 설명: 나눗셈 forward/backward 정확성
- 입력:
  - `a=Value(3.0)`, `b=Value(2.0)`
  - `y=a/b`, `y.backward()`
- 기대출력:
  - `y.data == 1.5`
  - `a.grad == 0.5`, `b.grad == -0.75`
- 판정기준(PASS/FAIL):
  - PASS: 세 값 모두 허용오차 내
  - FAIL: 하나라도 벗어남

### TC-V005
- ID: TC-V005
- 카테고리: Value autograd
- 설명: 거듭제곱 forward/backward 정확성
- 입력:
  - `x=Value(2.5)`
  - `y=x**3`, `y.backward()`
- 기대출력:
  - `y.data == 15.625`
  - `x.grad == 18.75` (`3*x^2`)
- 판정기준(PASS/FAIL):
  - PASS: forward/backward 모두 일치
  - FAIL: 불일치

### TC-V006
- ID: TC-V006
- 카테고리: Value autograd
- 설명: `tanh` forward/backward 정확성
- 입력:
  - `x=Value(0.7)`
  - `y=tanh(x)`, `y.backward()`
- 기대출력:
  - `y.data == tanh(0.7)`
  - `x.grad == 1 - tanh(0.7)^2`
- 판정기준(PASS/FAIL):
  - PASS: forward/backward 모두 수식값과 일치
  - FAIL: 불일치

### TC-V007
- ID: TC-V007
- 카테고리: Value autograd
- 설명: `exp` forward/backward 정확성
- 입력:
  - `x=Value(1.2)`
  - `y=exp(x)`, `y.backward()`
- 기대출력:
  - `y.data == exp(1.2)`
  - `x.grad == exp(1.2)`
- 판정기준(PASS/FAIL):
  - PASS: 두 값 모두 일치
  - FAIL: 불일치

### TC-V008
- ID: TC-V008
- 카테고리: Value autograd
- 설명: `log` forward/backward 정확성
- 입력:
  - `x=Value(2.5)`
  - `y=log(x)`, `y.backward()`
- 기대출력:
  - `y.data == ln(2.5)`
  - `x.grad == 1/2.5`
- 판정기준(PASS/FAIL):
  - PASS: 두 값 모두 일치
  - FAIL: 불일치

### TC-V009
- ID: TC-V009
- 카테고리: Value autograd
- 설명: 복합 체인 연산 `(a*b + c**2)` gradient 정확성
- 입력:
  - `a=Value(2.0)`, `b=Value(-3.0)`, `c=Value(4.0)`
  - `y = a*b + c**2`, `y.backward()`
- 기대출력:
  - `y.data == 10.0`
  - `a.grad == -3.0`, `b.grad == 2.0`, `c.grad == 8.0`
- 판정기준(PASS/FAIL):
  - PASS: forward 및 3개 gradient 모두 일치
  - FAIL: 하나라도 불일치

### TC-V010
- ID: TC-V010
- 카테고리: Value autograd
- 설명: `log(0 근처)` 수치 안정성 (`NaN/Inf` 방지)
- 입력:
  - `x=Value(1e-12)`
  - `y=log(x)` 후 `y.backward()`
  - 구현은 `x <= eps`에서 `eps` 클램프 적용
- 기대출력:
  - `y.data` 유한값
  - `x.grad` 유한값
- 판정기준(PASS/FAIL):
  - PASS: `isfinite(y.data)` and `isfinite(x.grad)`
  - FAIL: `NaN` 또는 `Inf` 발생

### TC-V011
- ID: TC-V011
- 카테고리: Value autograd
- 설명: `1/x (x→0)` 수치 안정성
- 입력:
  - `x=Value(1e-12)`
  - `y=1/x`, `y.backward()`
  - 구현은 분모 하한 `eps` 적용
- 기대출력:
  - `y.data` 유한값
  - `x.grad` 유한값
- 판정기준(PASS/FAIL):
  - PASS: 출력/gradient 모두 `NaN/Inf` 없음
  - FAIL: `NaN`/`Inf` 발생

### TC-V012
- ID: TC-V012
- 카테고리: Value autograd
- 설명: `relu` backward 경계값(`x=0`) 정의 확인
- 입력:
  - `x=Value(0.0)`
  - `y=relu(x)`, `y.backward()`
- 기대출력:
  - `y.data == 0.0`
  - `x.grad == 0.0` (경계 서브그라디언트 규칙)
- 판정기준(PASS/FAIL):
  - PASS: 출력 0, gradient 0
  - FAIL: 둘 중 하나라도 다름

---

## B. Loss 함수 (8)

### TC-L001
- ID: TC-L001
- 카테고리: Loss 함수
- 설명: 페그 완벽(`p=1`)이면 peg loss는 0
- 입력:
  - `p_t=1.0`, `lambda_p=5.0`
  - `peg_loss = lambda_p*(p_t-1)^2`
- 기대출력:
  - `peg_loss == 0.0`
- 판정기준(PASS/FAIL):
  - PASS: `peg_loss == 0`
  - FAIL: 0이 아님

### TC-L002
- ID: TC-L002
- 카테고리: Loss 함수
- 설명: 페그 이탈(`p=0.98`)이면 peg loss 양수
- 입력:
  - `p_t=0.98`, `lambda_p=5.0`
- 기대출력:
  - `peg_loss = 5*(0.02^2)=0.002 > 0`
- 판정기준(PASS/FAIL):
  - PASS: `peg_loss > 0` 및 계산식 일치
  - FAIL: 0 이하이거나 계산식 불일치

### TC-L003
- ID: TC-L003
- 카테고리: Loss 함수
- 설명: 담보비율 충분(`CR_t >= CR_min`)이면 CR penalty 0
- 입력:
  - `CR_min=1.20`, `CR_t=1.25`, `lambda_cr=20.0`
  - `cr_penalty=lambda_cr*max(0, CR_min-CR_t)^2`
- 기대출력:
  - `cr_penalty == 0.0`
- 판정기준(PASS/FAIL):
  - PASS: penalty 0
  - FAIL: 0이 아님

### TC-L004
- ID: TC-L004
- 카테고리: Loss 함수
- 설명: 담보비율 부족(`CR_t < CR_min`)이면 CR penalty 양수
- 입력:
  - `CR_min=1.20`, `CR_t=1.10`, `lambda_cr=20.0`
- 기대출력:
  - `cr_penalty = 20*(0.10^2)=0.2 > 0`
- 판정기준(PASS/FAIL):
  - PASS: `cr_penalty > 0` 및 계산값 일치
  - FAIL: 0 이하거나 불일치

### TC-L005
- ID: TC-L005
- 카테고리: Loss 함수
- 설명: 집중도(한 자산 100%)가 최대
- 입력:
  - `w=[1,0,0,0]`
  - `conc = sum(w_i^2)`
- 기대출력:
  - `conc == 1.0`
- 판정기준(PASS/FAIL):
  - PASS: 집중도 1.0
  - FAIL: 1.0 아님

### TC-L006
- ID: TC-L006
- 카테고리: Loss 함수
- 설명: 균등 분배(4자산) 집중도 최소
- 입력:
  - `w=[0.25,0.25,0.25,0.25]`
  - `conc=sum(w_i^2)`
- 기대출력:
  - `conc == 0.25`
- 판정기준(PASS/FAIL):
  - PASS: 집중도 0.25이며 TC-L005보다 작음
  - FAIL: 값 불일치 또는 비교 불만족

### TC-L007
- ID: TC-L007
- 카테고리: Loss 함수
- 설명: 턴오버 0(`w_t == w_{t-1}`)이면 turnover loss 0
- 입력:
  - `w_prev=[0.4,0.3,0.2,0.1]`
  - `w_t=[0.4,0.3,0.2,0.1]`
  - `turnover=lambda_turn*L1(w_t-w_prev)`
- 기대출력:
  - `turnover == 0.0`
- 판정기준(PASS/FAIL):
  - PASS: turnover 0
  - FAIL: 0 아님

### TC-L008
- ID: TC-L008
- 카테고리: Loss 함수
- 설명: 오라클 신뢰 `q=1.0`이면 oracle loss 0
- 입력:
  - `q_t=1.0`, `lambda_orc=3.0`
  - `oracle_loss=lambda_orc*(1-q_t)^2`
- 기대출력:
  - `oracle_loss == 0.0`
- 판정기준(PASS/FAIL):
  - PASS: oracle_loss 0
  - FAIL: 0 아님

---

## C. Optimizer (8)

### TC-O001
- ID: TC-O001
- 카테고리: Optimizer
- 설명: Adam 1스텝 후 파라미터가 실제로 변화
- 입력:
  - 초기 `theta=[0.40,0.30,0.20,0.10]`
  - gradient `g=[0.2,-0.1,0.05,-0.15]`
  - Adam 하이퍼파라미터 기본값
  - 1 step 업데이트 수행
- 기대출력:
  - 최소 1개 원소에서 `theta_new != theta_old`
- 판정기준(PASS/FAIL):
  - PASS: `any(abs(theta_new[i]-theta_old[i]) > 0)`
  - FAIL: 모든 원소 변화 없음

### TC-O002
- ID: TC-O002
- 카테고리: Optimizer
- 설명: simplex projection 후 가중치 합 1 불변식 유지
- 입력:
  - 비정규화 후보 `w_raw=[0.62,0.18,0.15,0.10]` (합 1.05)
  - projection 적용
- 기대출력:
  - `sum(w_proj) == 1.0`
- 판정기준(PASS/FAIL):
  - PASS: `abs(sum(w_proj)-1.0) <= atol`
  - FAIL: 합이 1 아님

### TC-O003
- ID: TC-O003
- 카테고리: Optimizer
- 설명: 개별 가중치 상한 초과 불가
- 입력:
  - 상한 `[0.55,0.45,0.45,0.35]`
  - 업데이트 후보 `w_raw=[0.70,0.20,0.05,0.05]`
- 기대출력:
  - projection 후 `w_i <= w_max_i` 모두 만족
- 판정기준(PASS/FAIL):
  - PASS: 모든 i에 대해 `w_proj[i] <= w_max[i]`
  - FAIL: 하나라도 초과

### TC-O004
- ID: TC-O004
- 카테고리: Optimizer
- 설명: 개별 가중치 하한(0) 미만 불가
- 입력:
  - 업데이트 후보 `w_raw=[0.50,-0.10,0.40,0.20]`
  - projection 적용
- 기대출력:
  - projection 후 `w_i >= 0` 모두 만족
- 판정기준(PASS/FAIL):
  - PASS: 최소값 `>=0`
  - FAIL: 음수 가중치 존재

### TC-O005
- ID: TC-O005
- 카테고리: Optimizer
- 설명: 스텝당 가중치 변화량 ±2% 제한
- 입력:
  - `w_prev=[0.40,0.30,0.20,0.10]`
  - `w_candidate=[0.46,0.24,0.20,0.10]` (의도적으로 ±6% 변화)
  - step delta cap 적용 (`0.02`)
- 기대출력:
  - 각 원소 `|w_new[i]-w_prev[i]| <= 0.02`
- 판정기준(PASS/FAIL):
  - PASS: 모든 i에서 변화량 제한 충족
  - FAIL: 하나라도 0.02 초과

### TC-O006
- ID: TC-O006
- 카테고리: Optimizer
- 설명: 수수료 변화량 ±10bps 제한
- 입력:
  - `mint_fee_prev=0.0020`
  - `mint_fee_candidate=0.0040` (20bps 증가 시도)
  - fee delta cap `0.001`
- 기대출력:
  - `|mint_fee_new - mint_fee_prev| <= 0.001`
- 판정기준(PASS/FAIL):
  - PASS: 변화량 10bps 이내
  - FAIL: 10bps 초과

### TC-O007
- ID: TC-O007
- 카테고리: Optimizer
- 설명: gradient clip norm 적용 확인
- 입력:
  - 원 gradient `g=[10.0,10.0,10.0,10.0]`
  - `clip_norm=1.0`
  - clip 함수 적용
- 기대출력:
  - `||g_clipped||_2 <= 1.0`
- 판정기준(PASS/FAIL):
  - PASS: 클리핑 후 norm 제한 만족
  - FAIL: norm > 1.0

### TC-O008
- ID: TC-O008
- 카테고리: Optimizer
- 설명: 100스텝 연속 실행 후 발산 없음
- 입력:
  - 동일 분포 샘플링 환경에서 optimizer 100 step 실행
  - 각 스텝마다 projection + clip 적용
- 기대출력:
  - 모든 스텝에서 loss, 파라미터가 유한값
  - 가중치 제약(`sum=1`, 범위) 지속 만족
- 판정기준(PASS/FAIL):
  - PASS: 100 step 전 구간 `NaN/Inf` 없음 + 제약 위반 없음
  - FAIL: 발산 또는 제약 위반 발생

---

## D. Circuit Breaker (10)

### TC-CB001
- ID: TC-CB001
- 카테고리: Circuit Breaker
- 설명: 단일 담보 디페그 시 CB-1 발동
- 입력:
  - USDT 가격 시퀀스: `[0.979, 0.978, 0.977]` (3스텝 연속 `|P-1|>0.02`)
  - 다른 자산 정상
- 기대출력:
  - `CB1.active == True`
  - 해당 담보 상한 50% 축소
  - 민팅 rate limit 적용 플래그 on
- 판정기준(PASS/FAIL):
  - PASS: 위 3개 상태 모두 충족
  - FAIL: 하나라도 미충족

### TC-CB002
- ID: TC-CB002
- 카테고리: Circuit Breaker
- 설명: 디페그 해소 시 CB-1 복구
- 입력:
  - CB-1 활성 상태에서 대상 담보 가격이 `0.995~1.005` 범위로 연속 안정
  - 복구 최소 유지시간 조건 충족
- 기대출력:
  - `CB1.active == False`
  - 상한/민팅 제한이 정상 정책으로 복귀
- 판정기준(PASS/FAIL):
  - PASS: CB-1 해제 + 정책 복원 확인
  - FAIL: 해제 실패 또는 정책 미복원

### TC-CB003
- ID: TC-CB003
- 카테고리: Circuit Breaker
- 설명: 다중 담보 동시 디페그 시 CB-2 발동
- 입력:
  - 동일 tick에서 USDC=0.97, USDT=0.96
  - 동시 디페그 자산 수 >= 2
- 기대출력:
  - `CB2.active == True`
- 판정기준(PASS/FAIL):
  - PASS: CB-2 활성화
  - FAIL: 미활성

### TC-CB004
- ID: TC-CB004
- 카테고리: Circuit Breaker
- 설명: CB-2 발동 시 민팅 중단
- 입력:
  - `CB2.active=True` 상태에서 `mint(request_amount)` 호출
- 기대출력:
  - mint 트랜잭션 거부(또는 amount=0 처리)
  - 상태코드 `MINT_PAUSED_BY_CB2`
- 판정기준(PASS/FAIL):
  - PASS: 민팅 성공하지 않음 + 중단 사유 코드 일치
  - FAIL: 민팅 허용되거나 코드 불일치

### TC-CB005
- ID: TC-CB005
- 카테고리: Circuit Breaker
- 설명: 오라클 stale이면 CB-3 발동
- 입력:
  - 마지막 갱신 시각과 현재 시각 차이 `> stale_limit`
  - 예: `stale_limit=120s`, 지연 `180s`
- 기대출력:
  - `CB3.active == True`
  - `oracle_degraded == True`
- 판정기준(PASS/FAIL):
  - PASS: 두 플래그 모두 활성
  - FAIL: 하나라도 미활성

### TC-CB006
- ID: TC-CB006
- 카테고리: Circuit Breaker
- 설명: CB-3 시 gradient 업데이트 중지
- 입력:
  - `CB3.active=True`
  - optimizer step 호출 전후 파라미터 기록
- 기대출력:
  - `theta_after == theta_before`
- 판정기준(PASS/FAIL):
  - PASS: 파라미터 변화 없음
  - FAIL: 파라미터가 바뀜

### TC-CB007
- ID: TC-CB007
- 카테고리: Circuit Breaker
- 설명: NaN 발생 시 CB-4 롤백
- 입력:
  - step t에서 loss 계산 중 `NaN` 강제 상황 주입
  - t-1 체크포인트 존재
- 기대출력:
  - 상태가 t-1 체크포인트로 복원
  - `CB4.active == True`
- 판정기준(PASS/FAIL):
  - PASS: 롤백 + CB4 활성 모두 확인
  - FAIL: 롤백 실패 또는 CB4 미활성

### TC-CB008
- ID: TC-CB008
- 카테고리: Circuit Breaker
- 설명: CB-4 후 learning rate 50% 축소
- 입력:
  - `lr_before=0.005`, CB-4 발생
- 기대출력:
  - `lr_after == 0.0025`
- 판정기준(PASS/FAIL):
  - PASS: `abs(lr_after - lr_before*0.5) <= atol`
  - FAIL: 비율 불일치

### TC-CB009
- ID: TC-CB009
- 카테고리: Circuit Breaker
- 설명: CB-1 + CB-3 동시 발생 시 충돌 없이 병행 활성
- 입력:
  - 단일 담보 디페그 조건 충족 + 동시에 오라클 stale 조건 충족
- 기대출력:
  - `CB1.active == True` and `CB3.active == True`
  - 우선순위 충돌로 인한 예외/중단 없음
- 판정기준(PASS/FAIL):
  - PASS: 두 breaker 동시 활성 + 상태머신 정상
  - FAIL: 한쪽 누락 또는 충돌 에러

### TC-CB010
- ID: TC-CB010
- 카테고리: Circuit Breaker
- 설명: on/off 진동 방지(최소 유지시간 히스테리시스)
- 입력:
  - 가격이 임계치 주변을 짧게 왕복하도록 tick 시퀀스 생성
  - `min_hold_ticks` 설정(예: 5)
- 기대출력:
  - breaker 활성 후 `min_hold_ticks` 이전에는 해제되지 않음
- 판정기준(PASS/FAIL):
  - PASS: 최소 유지시간 규칙 준수
  - FAIL: 조기 해제되어 진동 발생

---

## E. 시나리오 통합 (8)

### TC-S001
- ID: TC-S001
- 카테고리: 시나리오 통합
- 설명: 정상장 100틱에서 peg MAE 기준 만족
- 입력:
  - low-volatility 랜덤워크, 오라클 정상, tick=100
- 기대출력:
  - `peg_MAE < 0.0015`
- 판정기준(PASS/FAIL):
  - PASS: 임계값 미만
  - FAIL: 임계값 이상

### TC-S002
- ID: TC-S002
- 카테고리: 시나리오 통합
- 설명: 정상장에서 최종 CR이 목표치 초과
- 입력:
  - 정상장 시뮬레이션 종료 시점의 `CR_final`, `CR_target`
- 기대출력:
  - `CR_final > CR_target`
- 판정기준(PASS/FAIL):
  - PASS: 엄격 부등식 만족
  - FAIL: 이하

### TC-S003
- ID: TC-S003
- 카테고리: 시나리오 통합
- 설명: 단일 디페그 이벤트 후 페그/상태 복구
- 입력:
  - 중간 tick에 한 자산 -5% 충격 1회
  - 이후 정상화 구간 제공
- 기대출력:
  - CB-1 발동 후 해제까지 완료
  - `recovery_time <= SLA_ticks` (예: 30틱)
- 판정기준(PASS/FAIL):
  - PASS: 발동/복구 모두 확인 + SLA 충족
  - FAIL: 미복구 또는 SLA 초과

### TC-S004
- ID: TC-S004
- 카테고리: 시나리오 통합
- 설명: 다중 디페그에서도 solvency 유지
- 입력:
  - 2개 자산 동시 -8% 충격
  - 시뮬레이션 100틱
- 기대출력:
  - 전 구간 `CR_t >= CR_hard_min`
  - CB-2 동작 로그 존재
- 판정기준(PASS/FAIL):
  - PASS: hard min 위반 0회
  - FAIL: 1회 이상 위반

### TC-S005
- ID: TC-S005
- 카테고리: 시나리오 통합
- 설명: 고변동장에서도 발산 없음
- 입력:
  - 점프 빈도/변동성 상향된 가격 경로
  - 200틱 실행
- 기대출력:
  - loss/파라미터/CR 모두 유한값
  - 시스템 크래시 없음
- 판정기준(PASS/FAIL):
  - PASS: `NaN/Inf/예외종료` 없음
  - FAIL: 발산 또는 실행 중단

### TC-S006
- ID: TC-S006
- 카테고리: 시나리오 통합
- 설명: gradient 조작 공격 시 parameter drift 제한 내
- 입력:
  - 짧은 스파이크형 악성 가격 패턴 주입
  - 공격 구간 전후 파라미터 추적
- 기대출력:
  - 각 스텝 `|Δw_i| <= 0.02`, `|Δfee| <= 0.001`
- 판정기준(PASS/FAIL):
  - PASS: 전 스텝 제한 위반 0건
  - FAIL: 위반 1건 이상

### TC-S007
- ID: TC-S007
- 카테고리: 시나리오 통합
- 설명: 오라클 장애 발생 시 안전모드 전환
- 입력:
  - stale feed + source divergence 동시 발생
- 기대출력:
  - `CB3.active=True`
  - optimizer 비활성
  - 보수 프로파일(고CR/저민팅) 적용
- 판정기준(PASS/FAIL):
  - PASS: 3조건 모두 충족
  - FAIL: 하나라도 미충족

### TC-S008
- ID: TC-S008
- 카테고리: 시나리오 통합
- 설명: 오라클 복구 후 정상모드 복귀
- 입력:
  - TC-S007 이후 feed 신선도/편차 정상화 지속
- 기대출력:
  - `CB3.active=False`
  - optimizer 재활성
  - 동적 업데이트 재개
- 판정기준(PASS/FAIL):
  - PASS: 정상모드 플래그 복귀 확인
  - FAIL: 복귀 실패

---

## F. Agent 인터페이스 (4)

### TC-A001
- ID: TC-A001
- 카테고리: Agent 인터페이스
- 설명: Keeper proposal 제출 시 온체인 반영
- 입력:
  - keeper가 `submit_update_proposal` 호출
  - 제안값이 모든 bounded check/invariant 충족
- 기대출력:
  - proposal 상태 `APPROVED/APPLIED`
  - 온체인 파라미터가 제안값(또는 bounded 값)으로 갱신
- 판정기준(PASS/FAIL):
  - PASS: 트랜잭션 성공 + 상태/값 반영 확인
  - FAIL: 반영 누락 또는 실패

### TC-A002
- ID: TC-A002
- 카테고리: Agent 인터페이스
- 설명: Watchdog 이상 감지 시 CB 트리거
- 입력:
  - watchdog가 `depeg_detected` 또는 `oracle_stale` 이벤트 전송
- 기대출력:
  - 대응 breaker(`CB1/CB3`)가 즉시 활성화
  - 이벤트 로그에 감지자/사유 기록
- 판정기준(PASS/FAIL):
  - PASS: breaker 발동 + 감사로그 존재
  - FAIL: 미발동 또는 로그 누락

### TC-A003
- ID: TC-A003
- 카테고리: Agent 인터페이스
- 설명: Auditor가 invariant 위반 감지 시 alert 생성
- 입력:
  - 인위적으로 `sum(w)!=1` 또는 `CR<CR_hard_min` 상태 주입
  - auditor 점검 호출
- 기대출력:
  - `alert_emitted=True`
  - alert payload에 위반 invariant ID 포함
- 판정기준(PASS/FAIL):
  - PASS: alert 발송 및 페이로드 필드 완전
  - FAIL: alert 미발송 또는 필드 누락

### TC-A004
- ID: TC-A004
- 카테고리: Agent 인터페이스
- 설명: 수수료 분배 비율 정확성(30/10/5/55)
- 입력:
  - 총 수수료 `F=1000` 단위
  - 분배 규칙: `keeper/watchdog/auditor/reserve = 30/10/5/55`
- 기대출력:
  - 분배액 `[300,100,50,550]`
  - 합계가 정확히 `1000`
- 판정기준(PASS/FAIL):
  - PASS: 비율별 금액과 합계 모두 일치
  - FAIL: 금액/합계 중 하나라도 불일치

---

## 완료 체크리스트

- [x] 총 50개 TC 정의 완료
- [x] ID 체계 유지 (`TC-V/L/O/CB/S/A`)
- [x] 각 TC에 필수 항목 포함 (ID/카테고리/설명/입력/기대출력/판정기준)
- [x] 한글 작성
- [x] 구현 코드 없이 테스트 스펙만 작성
