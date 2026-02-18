---
name: functional-code-structure
description: "함수형 코드 구조 설계 원칙을 적용하는 스킬. 액션/계산/데이터 분리, 순수함수 설계, Copy-on-Write, 고차함수, 계층형 설계 등 함수형 프로그래밍 원칙에 따라 코드를 작성하거나 리팩토링하거나 코드 개선 및 점검할 때 사용한다."
version: improved-2026-02-18
---

코드를 작성하거나 리팩토링하거나 코드 개선 및 점검할 때 아래 절차를 **순서대로** 수행한다.  
원칙을 설명하지 말고, 코드에 직접 적용한다.

---

## STEP 0. 전역을 "데이터"와 "가변 상태"로 구분한다

전역이라고 해서 전부 액션으로 취급하지 않는다. 아래 기준으로 먼저 구분한다.

### 데이터(DATA)
아래 조건을 만족하면 **DATA** 로 취급한다(계산 함수에서 읽어도 된다).

- `const` 로 선언된 상수(환경 설정 값 포함)
- `Object.freeze(...)` 등으로 **불변이 보장된** 설정/룩업 테이블
- 빌드 타임에 고정되는 상수(예: 상수 맵, 정적 룰셋)

> 선택 규칙(엄격 모드): DATA도 외부 의존으로 보고 필요하면 인자로 올려도 된다.

### 가변 전역 상태(MUTABLE GLOBAL)
아래 중 하나라도 해당하면 **가변 전역 상태**이며, 직접 읽거나 쓰는 함수는 액션으로 취급한다.

- `let/var` 로 선언된 전역
- 객체/배열 전역(참조가 공유되고 변경될 수 있음)
- 외부에서 변경될 수 있는 모듈 상태

---

## STEP 1. 모든 함수를 액션/계산으로 분류한다

코드를 받으면 가장 먼저 각 함수를 분류한다.

### 1) 아래 패턴이 하나라도 있으면 → 액션(ACTION)

| 감지 패턴 | 이유 |
|---|---|
| 함수 본문에서 **가변 전역 상태**를 읽음 | 읽는 시점에 따라 값이 다름 |
| 함수 본문에서 **가변 전역 상태**를 변경 | 다른 코드에 영향을 줌 |
| `console.log`, `alert`, DOM 쓰기 | 외부 출력 |
| DOM 읽기, `new Date()`, `Math.random()` | 실행 시점에 결과가 달라짐 |
| Ajax/HTTP, DB 호출, 파일 I/O | 네트워크/외부 상태 의존 |
| 내부에서 다른 액션을 호출 | 액션을 호출하는 함수도 액션이 됨 |

### 2) 아래 패턴이 있으면 → "불순 계산(IMPURE CALC)"으로 표시한다
아래는 액션은 아니지만 순수 계산도 아니다. **STEP 2C로 반드시 변환해서 계산으로 만든다.**

| 감지 패턴 | 이유 |
|---|---|
| 인자로 받은 배열/객체를 **in-place로 수정** (`push`, `pop`, `shift`, `splice`, `sort`, `reverse`, `obj[key]=...` 등) | 호출자가 가진 원본이 바뀜(암묵적 출력) |

### 3) 위 패턴이 없으면 → 계산(CALCULATION)
계산은 **건드리지 않는다**(단, "불순 계산"으로 표시된 경우는 예외).

---

## STEP 2. 액션 또는 불순 계산을 발견하면 아래 변환을 적용한다

### 변환 A: 전역변수 읽기 → 인자로 올리기

```js
// 감지: 함수 본문에서 가변 전역 상태를 직접 읽고 있다
function calc_total() {
  for (var i = 0; i < shopping_cart.length; i++) { // ← 전역변수 shopping_cart
    total += shopping_cart[i].price;
  }
}

// 변환: 전역변수를 인자로 받도록 시그니처를 바꾼다
function calc_total(cart) {                         // ← 인자로 올림
  var total = 0;
  for (var i = 0; i < cart.length; i++) {
    total += cart[i].price;
  }
  return total;
}
```

**규칙**: 가변 전역 하나당 인자 하나를 추가한다. 함수를 부르는 쪽에서 전역을 직접 넘긴다.

---

### 변환 B: 전역변수 변경 → return으로 바꾸기

```js
// 감지: 함수 본문에서 가변 전역 상태를 변경하고 있다
function add_item(name, price) {
  shopping_cart.push({ name, price }); // ← 전역변수를 직접 변경
}

// 변환: 변경 대신 새 값을 리턴한다
function add_item(cart, name, price) { // ← 인자로 받고
  var new_cart = cart.slice();          // ← 복사하고
  new_cart.push({ name, price });       // ← 복사본을 수정하고
  return new_cart;                      // ← 리턴한다
}
// 부르는 쪽: shopping_cart = add_item(shopping_cart, name, price);
```

**규칙**: 함수가 외부 상태를 바꾸는 대신, 새 값을 리턴하게 만든다. 부르는 쪽이 리턴값을 받아서 변수에 대입한다.

---

### 변환 C: 배열/객체 수정(인자 mutation 포함) → Copy-on-Write로 바꾸기

인자로 받은 배열이나 객체를 직접 수정하는 코드를 발견하면 반드시 **복사 후 수정으로** 바꾼다.

**Copy-on-Write 3단계 공식: 복사 → 수정 → 리턴**

#### 배열인 경우
```js
// 감지: 인자로 받은 배열을 직접 수정하고 있다
function drop_first(array) {
  array.shift(); // ← 원본 변경
}

// 변환: slice()로 복사 → 복사본 수정 → 리턴
function drop_first(array) {
  var copy = array.slice();
  copy.shift();
  return copy;
}
```

#### 객체인 경우
```js
// 감지: 인자로 받은 객체를 직접 수정하고 있다
function setField(object, key, value) {
  object[key] = value; // ← 원본 변경
}

// 변환: Object.assign으로 복사 → 복사본 수정 → 리턴
function setField(object, key, value) {
  var copy = Object.assign({}, object);
  copy[key] = value;
  return copy;
}
```

#### 중첩 데이터(Shallow copy 함정) 처리 규칙
`slice()` / `Object.assign()` 은 **shallow copy** 이다.  
중첩 구조를 업데이트하는 경우, **변경 경로(path)의 모든 레벨을 Copy-on-Write** 해야 한다.

```js
// 예: cart.items[0].price 를 바꾸고 싶다 (중첩 업데이트)
function setFirstItemPrice(cart, price) {
  // 1) cart 복사
  var cartCopy = Object.assign({}, cart);

  // 2) items 배열도 복사
  var itemsCopy = cart.items.slice();

  // 3) 첫 item 객체도 복사
  var firstItemCopy = Object.assign({}, itemsCopy[0]);
  firstItemCopy.price = price;

  // 4) 조립
  itemsCopy[0] = firstItemCopy;
  cartCopy.items = itemsCopy;
  return cartCopy;
}
```

---

### 변환 D: 외부 라이브러리 호출 → 방어적 복사 적용

신뢰할 수 없는 외부 함수(서드파티/플러그인/SDK 등)에 데이터를 넘길 때는 **deepCopy** 로 입력/출력을 보호한다.

```js
// 감지: 외부/서드파티 함수에 내부 데이터를 그대로 넘기고 있다
function process(data) {
  return externalLib(data); // ← 외부 함수가 data를 변경할 수 있음
}

// 변환: 입력과 출력 모두 deepCopy
function process(data) {
  var inputCopy = deepCopy(data);       // ← 입력 보호
  var result = externalLib(inputCopy);
  return deepCopy(result);              // ← 출력 보호
}
```

**규칙**
- deepCopy는 **서드파티 경계에서만** 사용한다(내부 계산 함수 호출에는 사용하지 않는다).
- deepCopy는 아래 구현을 우선한다.

```js
function deepCopy(x) {
  // 가능한 경우 structuredClone 사용
  if (typeof structuredClone === 'function') return structuredClone(x);

  // fallback: JSON (단, Date/Map/Set/함수/undefined/순환참조 등은 깨질 수 있음)
  return JSON.parse(JSON.stringify(x));
}
```

---

## STEP 3. 함수 이름에 암묵적 인수가 숨어 있으면 꺼낸다

**감지 방법**: 비슷한 함수가 여러 개 있고, 이름만 다르고 구현이 거의 같다면 이름 안에 인수가 숨어 있는 것이다.

```js
// 감지: 이름만 다르고 구현이 똑같다
function setPrice(cart, name, value) { ... }
function setQuantity(cart, name, value) { ... }
function setShipping(cart, name, value) { ... }

// 변환: 이름에서 다른 부분(price, quantity, shipping)을 인자로 꺼낸다
function setFieldByName(cart, name, field, value) { ... }
// 부르는 쪽: setFieldByName(cart, name, 'price', value)
```

---

## STEP 4. 반복되는 구조 패턴이 보이면 고차함수로 추출한다

**감지 방법**: 여러 함수에서 `slice()` + 수정 + `return`, 또는 `try/catch`, 또는 `for` 루프 구조가 반복된다.

### Copy 패턴이 반복될 때
```js
// 변환: withArrayCopy / withObjectCopy로 추출
function withArrayCopy(array, modify) {
  var copy = array.slice();
  modify(copy);
  return copy;
}

function withObjectCopy(object, modify) {
  var copy = Object.assign({}, object);
  modify(copy);
  return copy;
}

// 이후 모든 배열 Copy-on-Write 함수는 이걸 쓴다
function arraySet(array, idx, value) {
  return withArrayCopy(array, function(copy) { copy[idx] = value; });
}
```

### try/catch 패턴이 반복될 때
```js
function tryCatch(f, errorHandler) {
  try { return f(); }
  catch (error) { return errorHandler(error); }
}
```

### 필드 업데이트 패턴이 반복될 때
```js
// 감지: "값 읽기 → 변환 → objectSet으로 저장" 패턴이 반복된다
// 변환: update 함수로 추출
function update(object, key, modify) {
  return objectSet(object, key, modify(object[key]));
}

// 이후 incrementField, doubleField 등은 모두 update를 쓴다
function incrementField(item, field) {
  return update(item, field, function(v) { return v + 1; });
}
```

---

## STEP 5. 한 함수 안에 추상화 레벨이 다른 코드가 섞여 있으면 분리한다

**감지 방법**: 함수 안에 `for`, 배열 인덱스 같은 저수준 코드와, 비즈니스 의미가 있는 고수준 호출이 함께 있다.

```js
// 감지: for + 인덱스 탐색(저수준)과 removeItems 호출(고수준)이 섞여 있다
function remove_item_by_name(cart, name) {
  var idx = null;
  for (var i = 0; i < cart.length; i++) {   // ← 저수준
    if (cart[i].name === name) idx = i;
  }
  if (idx !== null) return removeItems(cart, idx, 1); // ← 고수준
}

// 변환: 저수준을 별도 함수로 분리한다
function indexOfItem(cart, name) {
  for (var i = 0; i < cart.length; i++) {
    if (cart[i].name === name) return i;
  }
  return null;
}

function remove_item_by_name(cart, name) {
  var idx = indexOfItem(cart, name);         // ← 같은 수준만 남음
  if (idx !== null) return removeItems(cart, idx, 1);
  return cart;
}
```

---

## STEP 6. DOM 업데이트와 비즈니스 로직이 섞여 있으면 분리한다

```js
// 감지: 계산 함수 안에 DOM 조작이나 log가 들어 있다
function add_item(cart, item) {
  log(item);                            // ← 액션: 이게 있으면 전체가 액션이 됨
  return objectSet(cart, item.name, item);
}

// 변환: 계산만 남기고, 액션은 호출하는 쪽으로 옮긴다
function add_item(cart, item) {
  return objectSet(cart, item.name, item); // ← 순수 계산
}
// 부르는 쪽에서:
log(item);
var newCart = add_item(cart, item);
```

---

## STEP 7. 비동기 함수의 암묵적 입출력을 제거한다

### (A) 콜백 스타일
```js
// 감지: 비동기 콜백 안에서 전역변수를 읽거나 DOM을 직접 업데이트한다
function doWork() {
  var total = 0;
  ajax(data, function() {
    total += data.length;
    update_dom(total);              // ← 암묵적 출력
  });
}

// 변환: 인자로 받고 콜백으로 결과를 넘긴다
function doWork(data, callback) {   // ← 인자로 받음
  var total = 0;
  ajax(data, function() {
    total += data.length;
    callback(total);               // ← 콜백으로 결과 전달
  });
}
// 부르는 쪽: doWork(data, update_dom);
```

### (B) async/await(Promise) 스타일
```js
// 감지: async 함수가 fetch 후 바로 DOM 업데이트(암묵적 출력)까지 한다
async function loadAndRenderCart() {
  const res = await fetch('/api/cart');
  const cart = await res.json();
  renderCart(cart); // ← 암묵적 출력
}

// 변환: async 액션은 "가져오기"까지만, 출력/렌더는 호출자가 담당
async function loadCart() {            // ← 액션(외부 I/O)
  const res = await fetch('/api/cart');
  return await res.json();
}

function cartTotal(cart) {             // ← 계산(순수)
  return cart.items.reduce((s, it) => s + it.price, 0);
}

// 부르는 쪽(액션 레이어)
async function main() {
  const cart = await loadCart();
  const total = cartTotal(cart);
  renderCart(cart);
  renderTotal(total);
}
```

---

## 변환 후 검증 체크리스트

변환을 마친 뒤 아래를 확인한다. 모두 "예"여야 한다.

| 확인 항목 | 판단 기준 |
|---|---|
| 계산 함수가 가변 전역 상태를 읽지 않는가? | 모든 입력이 인자로 들어와야 함(단, DATA는 예외 가능) |
| 계산 함수가 가변 전역 상태를 바꾸지 않는가? | 모든 출력이 return으로 나가야 함 |
| 계산 함수가 인자를 in-place로 수정하지 않는가? | 모든 업데이트는 Copy-on-Write(복사→수정→리턴) |
| 배열/객체 수정 전에 복사하고 있는가? | `slice` 또는 `Object.assign` 사용 |
| 중첩 업데이트 시 경로 전체를 복사하는가? | 바깥만 복사하고 안쪽을 공유하지 않음 |
| 외부(서드파티) 호출 시 deepCopy를 쓰는가? | 입력/출력 모두(경계에서만) |
| 액션 함수 안에 계산이 묻혀 있지 않은가? | 계산은 별도 함수로 분리 |
| 한 함수 안의 추상화 레벨이 균일한가? | 저수준 코드는 별도 함수로 분리 |
| DOM 조작이 비즈니스 로직과 분리됐는가? | 각각 다른 함수에 있어야 함 |
| 비슷한 함수가 여러 개 있다면 통합했는가? | 암묵적 인수를 명시적으로 올렸는지 확인 |
