# Plano de Implementação: Hephaestus (Monolithic Rust Architecture)

Este plano detalha o passo a passo para a construção do projeto **Hephaestus**, integrando rigorosamente três expertises fundamentais:
1. **Refatoração Contínua (`/refactor`)**: Foco em responsabilidade única, design patterns e código limpo desde a concepção (Small steps).
2. **Segurança por Padrão (`security-best-practices`)**: Isolamento estrito, proteção de memória, validação rigorosa de inputs e prevenção contra vulnerabilidades.
3. **Avaliação Agêntica Iterativa (`agentic-eval`)**: Integração de pipelines de autoavaliação (Evaluator-Optimizer) para garantir que as reparações da IA e a análise do AST sejam precisas e seguras.

---

## Fase 1: Setup Seguro, Escopo e Modelos de Domínio

**Objetivo:** Estruturar o alicerce do projeto em Rust com configuração rigorosa e limites claros.

*   **Passo 1.1: Inicialização e Gerenciamento de Dependências.**
    *   Executar `cargo new hephaestus`.
    *   Adicionar as dependências (`tokio`, `tree-sitter`, `rusqlite`, `serde`, etc.) no `Cargo.toml`.
    *   *Security*: Fixar as versões das bibliotecas (version pinning) para evitar ataques à cadeia de suprimentos (Supply Chain Attacks).
*   **Passo 1.2: Refatoração da Estrutura de Diretórios.**
    *   *Refactor*: Evite "God Modules". Crie os subdiretórios separando domínios (SRP): `src/interceptor/`, `src/telemetry/`, `src/ast/`, `src/memory/`, `src/investigation/`, `src/sandbox/`, e `src/orchestration/`.
*   **Passo 1.3: Tipagem Estrita e Modelos de Erro.**
    *   *Refactor*: Use *Primitive Obsession Fixes*. Crie o tipo unificado `HephaestusError` encapsulando erros do SQLite, Timeout, AST e Sandbox. Defina limites base (`Result<T, HephaestusError>`).

---

## Fase 2: Motor de Armazenamento e Análise de AST

**Objetivo:** Permitir persistência segura de "Genomas de Reparo" e análise nativa de código-fonte.

*   **Passo 2.1: `RepairGenomeStore` (In-process SQLite)**
    *   Implementar a conexão ao `~/.hephaestus/genomes.db`.
    *   *Security*: Prevenir SQL Injection usando obrigatoriamente `rusqlite::params!`. Validar o caminho do arquivo (`PathBuf` canonicalization) para evitar Path Traversal. Validação rígida nos hashes.
*   **Passo 2.2: `ASTAnalyzer` nativo (tree-sitter-rs)**
    *   Implementar parsing em 2 passos (extract structure + call graph).
    *   *Agentic Eval*: Construir um *Test-Driven Code Refinement Workflow*. Após extrair o "SlimNode", usar um avaliador para verificar se o ID determinístico se mantém consistente após pequenas alterações inofensivas no código.
    *   *Refactor*: Extrair a geração de Hashes SHA256 em um trait `DeterministicId` para evitar código duplicado.

---

## Fase 3: Isolamento Linux (Sandbox de Validação)

**Objetivo:** Garantir que o código proposto para reparo jamais comprometa o host.

*   **Passo 3.1: Configuração do `unshare` do Linux**
    *   Criar o builder para inicializar o subprocesso com namespaces isolados (user, mount, ipc, pid, uts, net).
    *   *Security*: O processo filho deve rodar com privilégios mínimos (Drop Capabilities). Jamais passar strings concatenadas diretamente a um shell; encadear os argumentos de forma atômica no `Command::new()`.
*   **Passo 3.2: Garantias de Timeout**
    *   Envolver a espera do subprocesso em `tokio::time::timeout`.
    *   *Security / Refactor*: Tratar falhas aplicando *Guard Clauses*. Garantir que pânicos no subprocesso acionem o `kill()` e reaproveitem zombies (reaping) de maneira implacável.

---

## Fase 4: O Protocolo de Investigação de 7 Fases e Máquina de Estados

**Objetivo:** Traduzir a lógica de debug humano para um pipeline rígido validado em tempo de compilação.

*   **Passo 4.1: Modelagem com Type-States**
    *   Escrever `InvestigationPhase` enclausurado no sistema de tipos do Rust. Cada uma das 7 fases (Problema, Reprodução, Evidência, Hipótese, Guarda, Correção, Verificação) só pode ser superada ao cumprir uma *Hard Gate*.
    *   *Refactor*: Substituir validações aninhadas (*Arrow Code*) por Guard Clauses na função `validate()`. Eliminar *Feature Envy* movendo a lógica pertinente para dentro das próprias Structs de Fase.
*   **Passo 4.2: Proposta de Mutação Agêntica**
    *   Construir a ponte onde o LLM atua.
    *   *Agentic Eval*: Integrar um **"LLM-as-Judge Evaluation System"** no gate 6. Antes de executar o código no Sandbox, o agente auto-certifica os fixes usando uma "Rubrica de Qualidade": a correção é mínima? Quebra outra interface? Se falhar, é rejeitada sem sequer acessar o Sandbox.

---

## Fase 5: Telemetria "Time-Travel" e a Camada Interceptadora

**Objetivo:** Obter contexto profundo sem travar o agente principal e de forma segura.

*   **Passo 5.1: Leitura de Memória e Stack Trace**
    *   Implementar o `TimeTravelTelemetry` (Captura de `/proc/self/maps` e leitura de frames com `backtrace`).
    *   *Security*: Os blocos `unsafe` para ler registradores CPU precisam de checagens extremas de limites. Assegurar que nenhum dado sensível do Heap (ex: chaves privadas) possa vazar para logs externos que não sejam o arquivo estrito do banco local.
*   **Passo 5.2: O Hook `HephaestusInterceptor`**
    *   Implementar a macro/função panicking catcher `execute_skill_with_interception`.
    *   *Refactor*: Desacoplar relatórios de logs via um `mpsc::channel` (`HephaestusEvent`) para impedir que a tarefa principal (Main Loop) crie bloqueios (I/O).

---

## Fase 6: Orquestração do Agente Bifurcado (The Core Loop)

**Objetivo:** Paralelizar o reparo via execução tokio, isolando investigações do ciclo de vida crítico.

*   **Passo 6.1: `BifurcatedAgent` Main Pipeline**
    *   Utilizar `tokio::spawn` para desviar falhas (`RepairTrigger`).
    *   Encadear as sub-tarefas: Extração (AST) -> Grafo -> Protocolo de 7 Fases -> Proposta (Eval) -> Execução Sandbox -> Storage.
*   **Passo 6.2: Controle de Concorrência**
    *   *Security / Refactor*: Aplicar Semáforos (`tokio::sync::Semaphore`) no limite de `max_parallel_repairs`. Sem isso, skills que entram em loop ou colapsam simultaneamente causariam esgotamento de memória e negação de serviço (DoS) local (Limitado aos 45 MiB exigidos no perfil de agente).

---

## Fase 7: Otimização e Ciclo de Polimento (End-to-End)

**Objetivo:** Provar a robustez ("Zero-Trust") de todo o monolito em Rust.

*   **Passo 7.1: Setup dos "Evaluator-Optimizer Pipelines"**
    *   *Agentic Eval*: Rodar scripts simuladores injetando `NullPointerExceptions` intencionais. Usar logs não só para debugar, mas como dataset para quantificar a taxa de "Test Pass Rate" (percentual das correções agênticas que passam no ciclo).
*   **Passo 7.2: Refatoração Cirúrgica Final**
    *   Executar o `cargo clippy -- -D warnings`.
    *   Remover todo "*Dead Code*" criado na prototipação. 
    *   Assegurar a inexistência de "Magic Strings/Numbers" centralizando códigos de constantes (como Tempo de Sandbox, Thresholds de Confiança, Nomes de Arquivos) em Structs de Configuração do próprio `.toml`.