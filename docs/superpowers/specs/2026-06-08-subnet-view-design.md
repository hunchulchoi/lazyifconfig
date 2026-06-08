# 서브넷 그룹 뷰(Subnet Group View) 설계 문서

- **작성일**: 2026-06-08
- **상태**: 사용자 리뷰 및 승인 대기

## 개요

`lazyifconfig`는 네트워크 인터페이스를 개별적으로 관찰하는 기존의 인터페이스 뷰 외에도, 서브넷(Subnet) 단위로 인터페이스들을 묶어서 볼 수 있는 **네트워크 뷰(Network View)**를 새롭게 제공합니다. 이를 통해 개발자는 VPN 주소 대역, Docker 가상 네트워크, 로컬 LAN 대역을 한눈에 파악하고 관련 이슈를 쉽게 디버깅할 수 있습니다.

---

## 1. 요구 사항 및 핵심 원칙

1. **IP 대역 기반의 정확한 그룹화**:
   - 단순히 넷마스크(Netmask)가 같다고 묶는 것이 아니라, `IP 주소 + 넷마스크(Prefix Length)`를 연산하여 도출된 **네트워크 주소(Network Address)**와 **접두사 길이(Prefix Length)**가 완전히 일치하는 인터페이스들을 하나의 서브넷 그룹으로 묶습니다.
   - 예: `192.168.0.15/24`와 `192.168.0.200/24`는 동일한 `192.168.0.0/24` 네트워크로 그룹화되지만, `10.0.0.5/24`는 다른 그룹으로 분류됩니다.

2. **IPv4 및 IPv6 서브넷 모두 지원**:
   - IPv4와 IPv6 주소 대역 모두 서브넷 추출 및 그룹화를 지원합니다.
   - IP 주소가 아예 없거나 비활성화된 인터페이스는 `Unassigned` (IP 없음) 그룹으로 묶어 최하단에 노출시킴으로써 정보 누락이 없도록 합니다.

3. **키보드 탐색 일관성 유지**:
   - 단축키 `i`를 누르면 인터페이스 뷰(Interface View), `n`을 누르면 네트워크 뷰(Network View)로 전환됩니다.
   - 네트워크 뷰에서는 서브넷 헤더와 개별 인터페이스 목록 모두 `j/k` 및 방향키로 선택 가능합니다.
   - 서브넷 헤더 선택 시: 해당 서브넷 정보(네트워크 주소, 마스크, 소속 인터페이스 요약)를 우측 디테일 패널에 표시합니다.
   - 인터페이스 선택 시: 기존과 동일한 인터페이스 상세 정보(MAC, MTU, 입출력 통계 및 초당 전송 속도 등)를 우측 디테일 패널에 표시합니다.
   - 뷰 모드 전환 시, 기존에 선택하고 있던 인터페이스 이름 기반으로 선택 위치가 보존되도록 구현합니다.

---

## 2. 세부 설계

### A. 데이터 모델 정의 및 변경

#### [InterfaceAddress] (변경)
`src/model.rs` 파일의 `InterfaceAddress` 구조체에 `prefix_len` 옵션 필드를 추가합니다.

```rust
// src/model.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub value: String,
    pub prefix_len: Option<u8>,
}
```

#### [Subnet] (신규)
서브넷 그룹을 식별하고 정렬하기 위해 `Subnet` 열거형을 새로 정의합니다.

```rust
// src/model.rs
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Subnet {
    Ipv4 {
        network: std::net::Ipv4Addr,
        prefix_len: u8,
    },
    Ipv6 {
        network: std::net::Ipv6Addr,
        prefix_len: u8,
    },
    Unassigned,
}
```

#### [NavigationItem] (신규)
탐색 인덱스가 가리킬 수 있는 대상을 단일화하기 위해 `NavigationItem` 열거형을 정의합니다.

```rust
// src/app.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavigationItem {
    Interface { name: String, associated_ip: String },
    SubnetHeader(Subnet),
}
```

---

### B. 파서(Parser) 및 서브넷 계산 헬퍼 구현

#### `ifconfig` 파서 처리 수정 (`src/collector/interface.rs`)
- `inet` 파싱 시 `netmask 0x...` 부분을 읽고, `0x` 뒤의 16진수 값을 `u32`로 파싱하여 1의 개수(`count_ones()`)를 세어 `prefix_len`을 계산합니다.
- `inet6` 파싱 시 `prefixlen ...` 부분을 읽어 해당 숫자를 `u8`로 파싱합니다.

#### 서브넷 연산 구현 (`src/model.rs` 또는 헬퍼 모듈)
- **IPv4**: `u32`로 변환된 IP 주소와 `u32::MAX << (32 - prefix_len)`의 비트 AND 연산을 통해 네트워크 주소를 구합니다.
- **IPv6**: `[u8; 16]` 주소 octets를 순회하며 접두사 길이에 상응하는 마스크 바이트를 적용하여 네트워크 주소를 구합니다.

---

### C. App State 관리 및 탐색 로직

`src/app.rs`에 `view_mode`와 `navigation_items` 필드를 추가하고 아래 로직을 작성합니다.

```rust
// src/app.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Interface,
    Network,
}

// App 구조체에 추가
pub struct App {
    ...
    pub view_mode: ViewMode,
    pub navigation_items: Vec<NavigationItem>,
}
```

#### `update_navigation_items(&mut self)`
현재 `view_mode`에 맞추어 `navigation_items`를 빌드합니다.
- `ViewMode::Interface`:
  - `current_snapshot` 내 인터페이스들을 순회하며 `NavigationItem::Interface` 목록으로 그대로 채웁니다.
- `ViewMode::Network`:
  - 모든 인터페이스를 순회하여 각 IP 주소에 상응하는 `Subnet`들을 계산하고, 서브넷 단위로 그룹화합니다.
  - 서브넷을 `IPv4 -> IPv6 -> Unassigned` 순서로 정렬합니다.
  - 정렬된 서브넷마다 `SubnetHeader`를 추가한 후 소속 인터페이스 리스트(`Interface { name, associated_ip }`)를 연이어 추가합니다.

#### 선택 이동 및 보존 로직
- `select_next()` / `select_previous()`: `navigation_items` 리스트 위에서 `selected_index`를 순환 이동시킵니다.
- `set_view_mode(&mut self, mode: ViewMode)`:
  - 뷰 모드를 전환하기 전 현재 가리키던 인터페이스의 이름을 수집합니다.
  - 새로운 뷰 모드로 `navigation_items`를 재빌드합니다.
  - 이름이 일치하는 인터페이스 항목을 찾아서 `selected_index`를 설정하고, 찾지 못했다면 `selected_index = 0`으로 포백합니다.

---

## 3. UI 및 렌더링 세부 사항 (`src/ui.rs`)

1. **상태 표시줄 단축키 추가**:
   - `i: Interface View`, `n: Network View` 전환 힌트를 제공합니다.

2. **좌측 목록 패널 렌더링**:
   - `navigation_items`를 순회하며 렌더링합니다.
   - **`SubnetHeader`**:
     - 볼드 및 Cyan(청록색) 스타일로 `192.168.0.0/24 (IPv4)` 형태로 출력합니다.
     - 선택 중인 경우 노란색 볼드 등으로 강조합니다.
   - **`Interface`**:
     - Network View에서는 2칸 들여쓰기하여 `  ● en0 (192.168.0.15)` 형태로 출력합니다.
     - 선택 중인 경우 하이라이트 스타일을 적용합니다.

3. **우측 디테일 패널 렌더링**:
   - `selected_index`가 `NavigationItem::SubnetHeader`를 가리킬 때 서브넷 상세를 출력합니다:
     ```text
     Subnet: 192.168.0.0/24 (IPv4)

     Subnet Details:
       Network Address: 192.168.0.0
       Prefix Length:   24
       Netmask:         255.255.255.0

     Member Interfaces:
       - en0 (192.168.0.15)
       - bridge0 (192.168.0.1)
     ```
   - 인터페이스를 가리킬 때는 기존과 같이 인터페이스 상세 요약을 출력합니다.

---

## 4. 검증 계획

### 자동화 테스트 (`cargo test`)
1. **서브넷 파싱 및 연산 테스트**:
   - IPv4 및 IPv6 `ifconfig` raw output 파싱 테스트 케이스 추가.
   - 서브넷 마스크 연산 및 네트워크 주소 산출 정확성 검증.
2. **App 탐색 아이템 정렬 및 빌드 테스트**:
   - 스냅샷 갱신 후 `navigation_items`가 기대하는 정렬 기준대로 구성되었는지 확인.
   - 뷰 모드 전환 시 이전 선택된 인터페이스 이름이 올바르게 보존되는지 검증.
3. **UI 무패닉 테스트**:
   - 서브넷 헤더 및 인터페이스 선택 상태에서 `ui::draw`가 패닉 없이 정상 드로우되는지 검증.
