# 서브넷 그룹 뷰 & 네트워크 분류 설계 문서

- **작성일**: 2026-06-08
- **상태**: 사용자 리뷰 및 승인 대기

## 개요

`lazyifconfig`는 복잡한 네트워크 환경에서 개발자가 각 인터페이스의 본질적인 역할과 전체적인 토폴로지를 쉽게 파악할 수 있도록 두 가지 핵심 고도화 기능을 도입합니다.

1. **네트워크 분류 (Network Classification)**: 각 인터페이스가 가진 IP 대역과 이름을 분석하여 `LAN`, `VPN`, `CONTAINER` 등 7가지 유형 중 하나로 역할을 선제 정의하고 UI에 노출합니다.
2. **서브넷 그룹 뷰 (Subnet Group View)**: 평평한 인터페이스 목록 대신, 서브넷(Subnet) 대역 단위로 그룹화된 네트워크 중심의 뷰를 제공합니다.

---

## 1. 요구 사항 및 핵심 원칙

### A. 네트워크 분류 (Network Classification)

인터페이스의 역할 분류는 다음의 우선순위 규칙에 따라 수행되며, Heuristic에 근거해 실시간 분류됩니다.

```text
Loopback (루프백)
↓
VPN (가상 사설망)
↓
Container (컨테이너 네트워크)
↓
Link Local (링크 로컬)
↓
Public (공인 IP)
↓
LAN (사설 IP)
↓
Unknown (분류 실패)
```

1. **Loopback**: 이름이 `lo0`/`lo`로 시작하거나, IP 주소가 `127.0.0.0/8` 혹은 `::1`인 경우.
2. **VPN**: 이름이 `utun`, `tun`, `tap`, `wg`로 시작하는 경우. (VPN은 LAN 대역을 자주 사용하므로 LAN 판정보다 우선순위가 높아야 함)
3. **Container**: 이름이 `docker`, `bridge`, `br-`로 시작하는 경우.
4. **Link Local**: IP 주소가 `169.254.0.0/16` 대역 혹은 `fe80::/10` 대역인 경우.
5. **Public**: IP 주소가 존재하고, 사설/루프백/링크로컬이 아닌 경우 (예: `8.8.8.8`).
6. **LAN**: IP 주소가 RFC1918 사설 대역(`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`) 혹은 RFC4193 고유 로컬 IPv6 대역(`fc00::/7`)인 경우.
7. **Unknown**: 위 규칙에 매칭되는 정보가 없는 경우.

### B. 서브넷 그룹 뷰 (Subnet Group View)

1. **IP 대역 기반의 정확한 그룹화**:
   - 단순히 Netmask 문자열 일치 여부가 아닌, `IP 주소 + Netmask(Prefix Length)` 비트 연산으로 도출된 **네트워크 주소(Network Address)**와 **접두사 길이(Prefix Length)**가 완전히 일치하는 인터페이스들을 하나의 서브넷 그룹으로 묶습니다.
   - 예: `192.168.0.15/24`와 `192.168.0.200/24`는 동일한 `192.168.0.0/24` 네트워크로 그룹화됩니다.
2. **IPv4 및 IPv6 서브넷 모두 지원**:
   - IPv4와 IPv6 주소 대역 모두 서브넷 추출 및 그룹화를 지원합니다.
   - IP 주소가 아예 없거나 비활성화된 인터페이스는 `Unassigned` (IP 없음) 그룹으로 묶어 최하단에 노출시킵니다.
3. **선택 상태 보존 및 탐색**:
   - 단축키 `i`로 기존 인터페이스 뷰, `n`으로 네트워크 뷰를 토글할 수 있습니다.
   - 뷰 토글 시 선택한 인터페이스 이름 기준으로 커서 위치가 부드럽게 유지됩니다.

---

## 2. 세부 설계 및 코드 모델 변경

### A. 데이터 모델 정의 및 변경

#### [NetworkKind] (신규 - `src/model.rs`)
역할 분류를 표현하는 열거형을 추가합니다.

```rust
// src/model.rs
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkKind {
    Loopback,
    Lan,
    Vpn,
    Container,
    LinkLocal,
    Public,
    Unknown,
}

impl NetworkKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NetworkKind::Loopback => "LOOPBACK",
            NetworkKind::Lan => "LAN",
            NetworkKind::Vpn => "VPN",
            NetworkKind::Container => "CONTAINER",
            NetworkKind::LinkLocal => "LINK LOCAL",
            NetworkKind::Public => "PUBLIC",
            NetworkKind::Unknown => "UNKNOWN",
        }
    }
}
```

#### [InterfaceAddress] (변경 - `src/model.rs`)
주소 모델에 `prefix_len` 필드를 추가합니다.

```rust
// src/model.rs
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceAddress {
    pub value: String,
    pub prefix_len: Option<u8>,
}
```

#### [NetworkInterface] (변경 - `src/model.rs`)
기존 UI 전용의 `interface_type` 필드를 비즈니스 분류 레이어인 `network_kind` 필드로 승격 또는 보완합니다.

```rust
// src/model.rs
pub struct NetworkInterface {
    pub name: String,
    pub network_kind: NetworkKind, // 신규 추가
    pub interface_type: InterfaceType, // (하위 호환 유지)
    pub status: InterfaceStatus,
    pub ipv4: Vec<InterfaceAddress>,
    pub ipv6: Vec<InterfaceAddress>,
    pub mac_address: Option<String>,
    pub mtu: Option<u32>,
    pub stats: Option<InterfaceStats>,
}
```

#### [Subnet] (신규 - `src/model.rs`)
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

#### [NavigationItem] (신규 - `src/app.rs`)
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

### B. 파서 및 서브넷/분류 계산 구현

#### 1. `ifconfig` 파서 처리 수정 (`src/collector/interface.rs`)
- `inet` 파싱 시 `netmask 0x...` 부분을 읽고, `0x` 뒤의 16진수 값을 `u32`로 파싱하여 1의 개수(`count_ones()`)를 세어 `prefix_len`을 계산합니다.
- `inet6` 파싱 시 `prefixlen ...` 부분을 읽어 해당 숫자를 `u8`로 파싱합니다.
- 인터페이스 파싱이 완료되면 `classify_interface` 함수를 호출하여 `network_kind`를 산출해 저장합니다.

#### 2. 서브넷 연산 헬퍼 구현
- **IPv4**: `u32::MAX << (32 - prefix_len)`의 비트 AND 연산을 통해 네트워크 주소를 구합니다.
- **IPv6**: `[u8; 16]` octets를 순회하며 접두사 길이에 상응하는 마스크 바이트를 적용하여 네트워크 주소를 구합니다.

---

### C. App State 관리 및 탐색 로직

`src/app.rs`에 `view_mode`와 `navigation_items` 필드를 추가하고 아래 로직을 구현합니다.

```rust
// src/app.rs

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Interface,
    Network,
}

// App 구조체에 필드 추가
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

1. **인터페이스 뷰 목록 레이아웃 변경**:
   - `network_kind` 분류 텍스트를 우측 컬럼에 추가하여 출력합니다.
   - 예: `● en0 (192.168.0.15)       LAN`

2. **네트워크 뷰 목록 레이아웃**:
   - **`SubnetHeader`**:
     - 볼드 및 Cyan(청록색) 스타일로 `192.168.0.0/24 (IPv4)` 형태로 출력합니다.
   - **`Interface`**:
     - 2칸 들여쓰기하고 우측 정렬된 분류명을 배치하여 `  ● en0 (192.168.0.15)     LAN` 형태로 출력합니다.

3. **우측 디테일 패널 렌더링**:
   - **인터페이스 선택 시**: 상세 속성에 `Classification: LAN` 과 같이 추가 필드를 노출합니다.
   - **서브넷 헤더 선택 시**: 서브넷 상세를 출력합니다.
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

---

## 4. 검증 계획

### 자동화 테스트 (`cargo test`)
1. **분류 규칙 및 서브넷 파싱 연산 테스트**:
   - IPv4 및 IPv6 `ifconfig` raw output 파싱 테스트 케이스 추가.
   - 서브넷 마스크 연산 및 네트워크 주소 산출 정확성 검증.
   - 분류 우선순위 규칙이 올바르게 동작하는지 단위 테스트 추가 (예: `utun4`에 `10.8.0.2` 주소가 붙어 있어도 VPN으로 판단하는지).
2. **App 탐색 아이템 정렬 및 빌드 테스트**:
   - 스냅샷 갱신 후 `navigation_items`가 기대하는 정렬 기준대로 구성되었는지 확인.
   - 뷰 모드 전환 시 이전 선택된 인터페이스 이름이 올바르게 보존되는지 검증.
3. **UI 무패닉 테스트**:
   - 서브넷 헤더 및 인터페이스 선택 상태에서 `ui::draw`가 패닉 없이 정상 드로우되는지 검증.
