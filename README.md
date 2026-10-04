# Backpack

어디서든 내 AI 에이전트 환경을 꺼내 쓰세요.

Backpack은 macOS·Windows·Linux에서 AI 코딩 에이전트의 스킬, 룰, 커스텀 tools, hooks, MCP 연결, 플러그인과 지원되는 설정을 탐지하고 관리하며 동기화하는 오픈소스 데스크톱 앱을 목표로 합니다.

자신의 저장소를 연결하고, 각 PC에 실제로 설치된 항목을 확인한 뒤, 공유하고 적용할 항목을 선택합니다. 공유 라이브러리에 없는 로컬 설치 항목도 확인할 수 있습니다.

설정 파일 탐지와 Codex App Server 직접 조회를 함께 제공합니다. 각 결과의 근거와 조회 문맥을 표시합니다.

## 현재 상태

로컬 탐지·직접 조회와 일부 MCP 설정 변경을 구현했습니다. 에이전트 카탈로그의 알려진 사용자·프로젝트 설정 경로에서 스킬·룰·커스텀 tools·hooks·MCP·플러그인을 탐지합니다. 검색, 종류별·에이전트별 필터, 상세 정보와 사용자 지정 탐지 경로를 제공합니다. 탐지와 직접 조회는 에이전트 설정을 쓰지 않습니다. 별도 MCP 변경 화면에서 확인한 작업만 적용합니다. 앱 설정에서 시스템·라이트·다크 테마, 기본 제공 항목 표시, 시작 시 자동 탐지를 선택할 수 있습니다. 변경 즉시 적용되고 이 PC에 저장되며, 기본 테마는 시스템입니다. 확인된 기본 제공 항목은 기본 목록과 표시 개수에서 제외하며, 체크박스로 다시 표시할 수 있습니다. 현재 Codex 시스템 스킬 경로, 기본 제공 마켓플레이스와 실제 설치 경로가 확인된 node_repl 런타임을 구분합니다. 별도 설치한 공식 플러그인과 기본 플러그인의 비활성 설정은 표시합니다. 기본 파일의 원본 수정 여부는 아직 비교하지 않습니다.

Codex 직접 조회는 설치된 실행 파일의 별도 App Server 프로세스에서 적용 설정과 활성화된 스킬을 가져오고 파일 탐지 결과와 비교합니다. 선택적으로 MCP 인증 방식·실행 상태·제공 tool 이름도 조회합니다. MCP 조회는 서버 실행이나 외부 접속을 유발할 수 있어 기본으로 꺼져 있습니다. tool이나 모델은 호출하지 않습니다. 현재 열려 있는 Codex 대화의 세션 상태와는 별개이며, API가 실행 상태를 반환하지 않으면 미확인으로 표시합니다. 조회 취소와 제한 시간, 일부 실패 처리를 지원합니다.

추가 에이전트의 직접 조회·설정 변경, 개별 tool 실행 검사, 라이브러리·프로필과 클라우드 동기화는 후속 구현 대상입니다. 모든 설치 형태나 중첩된 룰·플러그인 캐시를 완전히 탐지하는 단계는 아닙니다. 브라우저 미리보기에서는 실제 로컬 탐지를 수행하지 않습니다.

탐지 카탈로그는 Codex·Antigravity·OpenCode·Claude Code·Gemini CLI·Cursor·GitHub Copilot CLI·Windsurf / Devin을 포함합니다. 실행 파일 또는 설정 리소스가 발견된 AI를 동적으로 표시하고, 미확인 목록도 펼쳐볼 수 있습니다. 기타 AI는 앱 설정에서 이름·설정 폴더·선택적 실행 파일·설정 파일 이름을 등록할 수 있으며, 등록은 이 PC에 저장됩니다. 기타 AI는 JSON·JSONC·TOML 설정과 공통 skills/rules/tools/plugins 폴더를 파일 기준으로 탐지합니다. 모든 프로그램을 이름만 보고 AI로 추정하지는 않습니다.

에이전트 카드를 선택하면 개별 설정 섹션과 필터가 열립니다. Codex 섹션은 App Server 직접 조회를 제공하며, Claude Code·OpenCode·Gemini CLI는 사용자가 선택해 `mcp list`로 MCP 연결 상태를 확인할 수 있습니다. 서버 실행·외부 접속 가능성을 표시하고, 서버 이름과 상태만 반환합니다. 파일 목록에 있는 MCP는 CLI 결과와 함께 표시하며, 목록에 없다고 비활성으로 판단하지 않습니다. 이 조회는 모델·개별 tool을 호출하지 않으며, 설정·스킬의 적용 상태나 tool 목록을 조회하는 명령은 아닙니다. 나머지는 파일 확인과 조회 지원 상태를 제공합니다.

CLI 조회에도 취소·45초 제한·출력 크기 제한·조회 프로세스 종료 관리를 적용합니다. Windows npm 설치는 알려진 패키지의 실행 파일이나 Node.js 진입점을 확인해 실행하며, 셸 래퍼를 해석하지 않습니다. 확인할 수 없는 설치 형태는 미지원으로 표시합니다. Gemini CLI의 사용자 지정 설정 루트는 파일 탐지만 지원합니다. Windows의 OpenCode 설치에서 실제 조회를 검증했고, Claude Code·Gemini CLI는 테스트용 출력과 프로세스로 검증했습니다. 구독이나 로그인 없이 설치·파일 탐지와 설정 비교를 검증할 수 있도록 설계합니다. 실제 모델 호출과 인증이 필요한 실행 검증은 별도로 구분합니다. 저장소는 로컬 폴더와 Google Drive 직접 연결을 우선 지원할 계획입니다. 첫 공개 버전에는 Backpack 전용 계정이나 운영 서버를 두지 않습니다.

## MCP 설정 변경

Codex·OpenCode·Claude Code·Antigravity·Gemini CLI의 개별 에이전트 섹션에서 새 MCP 등록·활성화·비활성화를 지원합니다. 로컬 명령·인자와 HTTP URL을 등록할 수 있고, 새 연결은 기본으로 비활성 설정을 저장합니다. 지원되는 환경 변수는 이름으로 참조하며 값은 가져오지 않습니다. Antigravity의 환경 변수 참조와 Antigravity·Gemini의 원격 토큰 참조 등록은 아직 지원하지 않습니다. 기존 인증 설정은 유지합니다. 변경 자체는 MCP 서버·모델·tool을 실행하지 않습니다. 실행 중인 에이전트에는 재시작·설정 재로드가 필요할 수 있습니다.

대상 파일 선택 → 변경 미리보기 → 백업하고 적용 순서로 진행합니다. 미리보기는 10분 뒤 만료되며, 적용 직전에 파일과 부모 폴더를 다시 검사합니다. 변경을 발견하면 중단하고 재확인을 요구합니다. TOML·JSONC 주석과 다른 설정을 유지하고, 같은 이름의 기존 서버를 새 등록으로 덮어쓰지 않습니다. 링크 파일과 해석할 수 없는 형식은 변경하지 않습니다.

원본과 복원 기록은 이 PC의 Backpack 앱 데이터 폴더에 저장합니다. Windows는 `%LOCALAPPDATA%/Backpack/backups`, macOS는 `~/Library/Application Support/Backpack/backups`, Linux는 `$XDG_DATA_HOME/Backpack/backups` 또는 `~/.local/share/Backpack/backups`입니다. 원본 백업에는 기존 파일의 인증 정보도 포함될 수 있으며, 레포·동기화 라이브러리에 넣지 않습니다. 적용 백업 목록에서 원본 복원을 확인할 수 있고, 현재 파일이 당시 적용 결과와 다르면 복원을 중단합니다. 새로 만든 파일을 복원하면 그 파일을 제거해 원래 상태로 되돌립니다.

현재 Codex의 `config.toml`과 프로젝트 `.codex/config.toml`, OpenCode의 `opencode.json`·`opencode.jsonc`와 절대 경로 `OPENCODE_CONFIG`를 지원합니다. 설정 폴더가 먼저 존재해야 합니다. OpenCode v1의 `mcp.<name>` 형식만 변경하며 v2의 `mcp.servers` 형식은 지원하지 않습니다. Codex의 인라인 MCP 테이블에 새 서버를 추가하는 작업도 아직 지원하지 않습니다. Claude는 프로젝트 폴더 선택 후 `~/.claude.json`의 해당 프로젝트 로컬 MCP를 등록합니다. 사용자·공유 프로젝트의 기존 MCP도 프로젝트별 `disabledMcpServers` 목록으로 전환하며, 공유 `.mcp.json` 정의 자체는 변경하지 않습니다. 사용자 지정 Claude 설정 루트는 쓰기 위치가 확인되지 않아 탐지만 지원합니다.

Antigravity는 `~/.gemini/config/mcp_config.json`과 프로젝트 `.agents/mcp_config.json`에서 `disabled`를 변경합니다. 원격 연결은 `serverUrl`로 등록합니다. 기존 `.agents/mcp.json`은 탐지 경로로 남기되 적용 여부를 미확인으로 표시합니다. Gemini는 사용자·프로젝트 `.gemini/settings.json`의 `mcp.excluded` 목록으로 전환하고, 로컬 환경 변수는 `${NAME}`, 원격 HTTP는 `httpUrl`로 등록합니다. 허용 목록은 자동으로 넓히지 않습니다. 파일 기준 활성 설정이 있어도 프로젝트 승인·다른 범위 설정·조직 정책이 실제 연결을 제한할 수 있습니다. 다른 에이전트의 쓰기는 후속 구현 대상입니다.

설정 규격: [Codex 설정](https://learn.chatgpt.com/docs/config-file/config-reference), [OpenCode MCP](https://opencode.ai/docs/mcp-servers/), [OpenCode 환경 변수 참조](https://opencode.ai/docs/config/), [Claude MCP와 프로젝트별 비활성 목록](https://code.claude.com/docs/en/mcp), [Antigravity MCP](https://antigravity.google/docs/mcp), [Gemini MCP](https://geminicli.com/docs/tools/mcp-server/).

## 설계

설계·개발 작업 문서는 로컬 `docs/`에서 관리하고 Git에서는 제외합니다. 공개 사용자 문서와 기여 가이드는 별도로 작성할 예정입니다.

이 레포는 앱 소스와 문서를 관리합니다. 사용자 라이브러리, PC별 설정, 인증 정보와 백업은 이 레포 밖에 저장합니다.

Tauri 2·React·TypeScript와 Rust 코어로 구현했습니다. Windows에서 로컬 탐지와 네이티브 빌드를 검증했으며, macOS·Linux 실행 검증은 남아 있습니다.

오픈소스 라이선스는 첫 공개 배포 전에 결정합니다. 현재 제안은 MIT입니다.

## 개발 실행

Node.js 22.12 이상과 Rust stable, [Tauri OS별 빌드 요구사항](https://v2.tauri.app/start/prerequisites/)이 필요합니다.

```sh
npm ci
npm run desktop
```

읽기 전용 탐지 코어는 GUI 없이도 실행할 수 있습니다.

```sh
npm run scan
npm run scan -- --project /absolute/path/to/project
```

검증과 빌드:

```sh
npm run build
cargo test -p backpack-core --locked
cargo build -p backpack-desktop --features custom-protocol --locked
```

Windows·macOS·Ubuntu용 CI 빌드·테스트를 구성했습니다. 실제 로컬 실행 검증은 Windows에서 진행하며, 다른 OS의 실행·패키징 검증은 별도로 완료해야 합니다. 설치 패키지 생성은 아직 활성화하지 않았습니다.

## 커스텀 tools

직접 만든 tool 정의와 구현 파일, 함께 호출하는 스크립트, 의존성, OS별 실행 설정과 권한을 관리 대상에 포함합니다. 저장소에 없는 로컬 tools도 탐지하며, MCP 서버 연결과 그 서버가 제공하는 개별 tool은 구분합니다. 에이전트별 도구 형식이 다르므로 지원 여부를 표시하고, 다른 에이전트로 자동 변환할 수 있다고 가정하지 않습니다.

직접 조회 명령: [Claude Code MCP](https://code.claude.com/docs/en/mcp), [OpenCode CLI](https://opencode.ai/docs/cli/), [Gemini CLI MCP](https://geminicli.com/docs/tools/mcp-server/).

참고한 설정 규격: [Gemini CLI](https://geminicli.com/docs/reference/configuration/), [Cursor MCP](https://prod.cursor.com/help/customization/mcp), [Copilot CLI](https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-config-dir-reference), [Devin Cascade MCP](https://docs.devin.ai/desktop/cascade/mcp). Windsurf의 기존 경로와 Devin의 현재 경로를 함께 확인합니다.

직접 만든 MCP 서버도 명령·인자 또는 지원되는 서버 URL로 등록할 수 있도록 설계합니다. 연결 정의, 선택적으로 공유할 서버 소스, PC별 경로·인증과 서버가 제공하는 tools를 구분해서 관리합니다.

이벤트에 따라 자동 실행하는 hooks도 관리합니다. 같은 기능이 에이전트마다 hook·플러그인·tool 등 다른 방식으로 연결될 수 있으므로, 공통 코드와 에이전트별 연결을 묶어서 표시합니다.
