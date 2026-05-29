# AstroFetch — Plano de Execução inspirado no screenFetch

## 1. Contexto do produto

O AstroFetch deve ser tratado como um aplicativo/CLI de terminal no estilo `screenFetch`, e não como jogo.

A ideia central é:

> Exibir informações úteis do sistema ao lado de uma imagem ASCII astrofísica dinâmica/procedural, com a possibilidade futura de incorporar um resumo do journal do sistema e fazer a arte reagir ao estado da máquina.

Referências de produto:

- `screenFetch`: ferramenta Bash que mostra informações do sistema junto a um logo ASCII da distribuição.
- `fastfetch`/`neofetch`: ferramentas modernas de terminal fetch.
- AstroFetch: variante científica/astrofísica em Rust, com imagem ASCII gerada proceduralmente em vez de logo fixo da distribuição.

## 2. Informações observadas na screenshot de referência

A screenshot enviada mostra um uso típico do `screenFetch`: logo ASCII à esquerda e informações do sistema à direita.

Campos exibidos na screenshot:

- `user@host`
- OS: Ubuntu 24.04 noble
- Kernel
- Uptime
- Packages
- Shell
- Resolution
- Desktop Environment
- Window Manager
- WM Theme
- GTK Theme
- Icon Theme
- Font
- Disk
- CPU
- GPU
- RAM

Esses campos devem orientar o primeiro alvo funcional do AstroFetch: antes de implementar um dashboard complexo, ele precisa reproduzir essa experiência básica com uma identidade visual própria.

## 3. Direção corrigida do projeto

O AstroFetch deve nascer como um equivalente astrofísico do screenFetch:

```bash
astrofetch
```

Saída esperada:

```text
      [ASCII astrofísico]        user@host
      [galáxia/aglomerado]       OS: Ubuntu 24.04 noble
      [procedural/dinâmico]      Kernel: 6.x
                                 Uptime: ...
                                 Packages: ...
                                 Shell: ...
                                 Resolution: ...
                                 DE: ...
                                 WM: ...
                                 Theme: ...
                                 Disk: ...
                                 CPU: ...
                                 GPU: ...
                                 RAM: ...
```

A diferença em relação ao screenFetch é que a imagem da esquerda não será um logo fixo da distribuição, mas uma estrutura astrofísica gerada proceduralmente:

- galáxia elíptica;
- galáxia espiral;
- aglomerado estelar;
- campo estelar;
- modo aleatório;
- futuramente, modo reativo ao estado do sistema.

## 4. Princípios de implementação

1. Primeiro entregar uma experiência screenFetch-like completa, ainda que simples.
2. Evitar TUI complexa no início.
3. Priorizar saída em stdout, rápida e estável.
4. Separar claramente:
   - geração astrofísica;
   - coleta de informações do sistema;
   - renderização ASCII;
   - composição/layout;
   - integração opcional com journal.
5. Não misturar a física da imagem com a coleta do sistema.
6. Não depender de `journalctl` no caminho principal do MVP.
7. O comando `astrofetch` deve funcionar mesmo sem systemd, sem GNOME e sem permissões especiais.
8. A arte deve ser bonita mesmo antes de ser cientificamente sofisticada.

## 5. Arquitetura recomendada

Estrutura sugerida:

```text
src/
  main.rs
  cli.rs
  app.rs
  error.rs

  astro/
    mod.rs
    canvas.rs
    model.rs
    elliptical.rs
    spiral.rs
    cluster.rs
    field.rs

  render/
    mod.rs
    ascii.rs
    color.rs
    palette.rs

  system/
    mod.rs
    snapshot.rs
    linux.rs
    desktop.rs
    packages.rs
    journal.rs

  layout/
    mod.rs
    line.rs
    compose.rs
    screenfetch.rs
```

Responsabilidades:

### `cli.rs`

Parseia argumentos com `clap`.

### `astro/`

Gera uma matriz 2D de intensidade ou uma lista de pontos/estrelas.

Não deve saber nada sobre CPU, RAM, kernel, usuário ou journal.

### `render/`

Converte a imagem numérica em linhas ASCII, com ou sem cor ANSI.

### `system/`

Coleta dados do sistema.

A primeira versão pode usar uma combinação de:

- `/proc`;
- comandos externos com timeout curto;
- crate `sysinfo`, se for conveniente;
- variáveis de ambiente para shell, terminal e desktop.

### `layout/`

Combina a imagem ASCII à esquerda com os campos de sistema à direita.

Essa é a camada responsável por manter alinhamento, padding, truncamento e fallback em terminal estreito.

### `app.rs`

Orquestra o fluxo:

1. ler CLI;
2. gerar arte;
3. coletar snapshot do sistema;
4. montar layout;
5. imprimir em stdout.

## 6. Interface CLI proposta

### Comando padrão

```bash
astrofetch
```

Mostra arte astrofísica + informações do sistema.

### Controles visuais

```bash
astrofetch --model spiral
astrofetch --model elliptical
astrofetch --model cluster
astrofetch --model random
astrofetch --width 40 --height 20
astrofetch --seed 42
astrofetch --no-color
astrofetch --palette dense
```

### Modos de saída

```bash
astrofetch --logo-only
astrofetch --info-only
astrofetch --layout right
astrofetch --layout portrait
astrofetch --compact
```

### Campos de sistema

Inspirado no `screenFetch -d`, permitir no futuro controlar campos exibidos:

```bash
astrofetch --fields os,kernel,uptime,shell,cpu,gpu,ram,disk
astrofetch --hide packages,resolution,font
```

### Journal opcional

```bash
astrofetch --journal
astrofetch --journal-lines 5
astrofetch --journal-unit sshd
astrofetch --journal-priority warning
```

O journal não deve estar no caminho principal do MVP inicial. Ele deve ser uma extensão opcional.

### Modo dinâmico futuro

```bash
astrofetch watch
astrofetch watch --interval 2s
astrofetch watch --journal --journal-lines 5
astrofetch watch --reactive
```

## 7. Roadmap de execução

### Fase 0 — Realinhar documentação

Objetivo: deixar claro que o AstroFetch é um fetch/dashboard de terminal, não um jogo.

Tarefas:

- Atualizar README com a visão screenFetch-like.
- Atualizar `docs/IMPLEMENTATION_ROADMAP.md` com fases de sistema, layout e journal.
- Adicionar screenshot/descrição de referência ao planejamento.
- Corrigir comandos de instalação para o repositório real.

Critério de pronto:

- README explica em uma frase o que o AstroFetch faz.
- Roadmap diferencia núcleo visual, painel de sistema e journal.

### Fase 1 — Vertical slice mínimo screenFetch-like

Objetivo: `astrofetch` já parecer um fetch real.

Tarefas:

- Criar projeto Rust com `cargo`.
- Adicionar `clap`.
- Criar uma arte ASCII astrofísica inicial, mesmo que simples.
- Coletar informações básicas:
  - user@host;
  - OS;
  - kernel;
  - uptime;
  - shell;
  - CPU;
  - RAM.
- Implementar layout lado a lado.
- Imprimir no stdout.

Critério de pronto:

```bash
cargo run --
```

produz uma saída parecida com screenFetch: imagem à esquerda, informações à direita.

Observação importante: nesta fase, a arte pode ser simples. O objetivo é validar a experiência completa.

### Fase 2 — Motor astrofísico procedural

Objetivo: substituir a arte simples por geração procedural.

Tarefas:

- Implementar `Canvas` 2D.
- Implementar modelo elíptico com perfil de Sérsic.
- Implementar modelo de campo estelar ou cluster.
- Implementar seed determinística.
- Implementar paleta ASCII.
- Garantir saída estável para mesma seed.

Critério de pronto:

```bash
astrofetch --model elliptical --seed 42
astrofetch --model cluster --seed 42
```

produzem imagens reprodutíveis e visualmente distintas.

### Fase 3 — Campos completos da screenshot

Objetivo: aproximar a informação exibida do screenFetch que você usa.

Tarefas:

- OS/distro detalhada.
- Kernel.
- Uptime.
- Packages, se viável.
- Shell.
- Resolution.
- DE.
- WM.
- WM Theme.
- GTK Theme.
- Icon Theme.
- Font.
- Disk.
- CPU.
- GPU.
- RAM.

Critério de pronto:

- O AstroFetch consegue exibir a maioria dos campos da screenshot em uma máquina Linux desktop.
- Campos indisponíveis aparecem como `Unknown` ou são omitidos sem quebrar o programa.

### Fase 4 — Opções de customização estilo screenFetch

Objetivo: permitir que o usuário controle saída, campos e visual.

Tarefas:

- `--no-color`.
- `--logo-only`.
- `--info-only`.
- `--compact`.
- `--layout portrait`.
- `--fields`.
- `--hide`.
- `--model`.
- `--seed`.
- `--palette`.

Critério de pronto:

- O usuário consegue reproduzir diferentes estilos de saída sem editar código.

### Fase 5 — Journal opcional

Objetivo: adicionar um resumo recente do journal do sistema sem comprometer o modo principal.

Tarefas:

- Implementar coleta via `journalctl` como comando externo.
- Usar timeout curto.
- Limitar linhas.
- Suportar filtros por unidade e prioridade.
- Tratar ausência de `journalctl`.
- Tratar erro de permissão.

Critério de pronto:

```bash
astrofetch --journal --journal-lines 5
```

mostra algumas linhas recentes ou uma mensagem amigável de indisponibilidade.

### Fase 6 — Arte reativa

Objetivo: fazer a imagem ASCII responder ao estado do sistema.

Ideias:

- CPU alta aumenta brilho/turbulência.
- RAM alta aumenta densidade.
- Load average altera ruído ou quantidade de estrelas.
- Erros recentes no journal alteram contraste ou adicionam marcadores visuais.

Critério de pronto:

```bash
astrofetch --reactive
```

produz uma imagem baseada no snapshot do sistema.

Regra importante:

- Se `--seed` for passado, a saída deve continuar reprodutível.
- O modo reativo deve ser opcional.

### Fase 7 — Modo watch

Objetivo: atualizar periodicamente a tela.

Tarefas:

- `astrofetch watch`.
- Intervalo configurável.
- Limpeza/redesenho simples do terminal.
- Encerramento seguro com Ctrl-C.
- Recoleta de snapshot do sistema.

Critério de pronto:

```bash
astrofetch watch --interval 2s
```

atualiza a tela sem flicker excessivo.

## 8. Dependências sugeridas

MVP inicial:

```toml
clap = { version = "4", features = ["derive"] }
rand = "0.8"
```

Possíveis dependências para sistema:

```toml
sysinfo = "0.30"
```

Possíveis dependências futuras para terminal dinâmico:

```toml
crossterm = "0.27"
```

Evitar no início:

```text
ratatui
```

Motivo: é excelente, mas pode transformar cedo demais um fetch simples em TUI complexa.

## 9. Riscos principais

### Risco 1 — Focar demais no motor físico antes do fetch funcionar

Mitigação:

- Implementar uma saída screenFetch-like mínima cedo.

### Risco 2 — Coleta de sistema virar um buraco sem fundo

Mitigação:

- Começar com poucos campos.
- Usar fallback para campos indisponíveis.
- Não tentar suportar todos os desktops na primeira versão.

### Risco 3 — Journal deixar o programa lento

Mitigação:

- `journalctl` apenas quando solicitado.
- Timeout curto.
- Limite de linhas.

### Risco 4 — Layout quebrar em terminais pequenos

Mitigação:

- Detectar largura do terminal futuramente.
- Ter modo compacto.
- Ter modo portrait.

### Risco 5 — Arte bonita vs. física correta

Mitigação:

- Priorizar estética no MVP.
- Manter inspiração física, não simulação rigorosa.

## 10. Primeiro milestone recomendado

O primeiro milestone deve ser uma fatia vertical:

> `astrofetch` imprime uma arte ASCII astrofísica simples à esquerda e informações reais do sistema à direita.

Escopo exato:

- CLI mínima.
- Arte inicial simples.
- Coleta de user, host, OS, kernel, uptime, shell, CPU e RAM.
- Layout lado a lado.
- Sem journal.
- Sem watch.
- Sem TUI.
- Sem física sofisticada.

Esse milestone valida o produto real rapidamente.

## 11. Prompt para o Gemini

Use este prompt em modo planejamento, antes de pedir implementação.

```text
You are helping me plan AstroFetch, a Rust terminal application inspired by screenFetch.

Important correction: AstroFetch is not a game. It is a terminal fetch/dashboard application.

The intended behavior is:
- show a procedural astrophysical ASCII image on the left;
- show system information on the right, similar to screenFetch;
- later support optional system journal lines;
- later support dynamic/watch mode;
- later allow the ASCII image to react to CPU/RAM/load/journal state.

Reference fields from my current screenFetch output:
- user@host
- OS
- Kernel
- Uptime
- Packages
- Shell
- Resolution
- DE
- WM
- WM Theme
- GTK Theme
- Icon Theme
- Font
- Disk
- CPU
- GPU
- RAM

Current AstroFetch roadmap already includes:
- Rust CLI with clap;
- rand for Monte Carlo;
- model selection: Random, Elliptical, Spiral, Cluster;
- width, height, sersic options;
- physical engine producing a 2D f64 luminosity matrix;
- ASCII renderer;
- main integration printing to stdout without panics.

Please revise the implementation plan with this product direction.

I want you to:
1. Propose the smallest useful vertical slice.
2. Recommend a Rust module architecture.
3. Decide which system fields to implement first.
4. Keep the visual engine independent from system collection.
5. Avoid a full TUI framework at first.
6. Treat journal integration as optional and later.
7. Prefer stdout output compatible with a screenFetch-like workflow.
8. Do not implement yet. Return a concise technical execution plan.
```

## 12. Comando sugerido para adicionar este plano ao repositório

Salve este arquivo como:

```text
docs/SCREENFETCH_EXECUTION_PLAN.md
```

Comando local:

```bash
cp astrofetch_screenfetch_execution_plan.md docs/SCREENFETCH_EXECUTION_PLAN.md
```

Depois valide:

```bash
git diff --check
git status --short
git diff -- docs/SCREENFETCH_EXECUTION_PLAN.md
```
