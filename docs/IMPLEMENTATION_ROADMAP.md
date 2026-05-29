# AstroFetch - Implementation Roadmap

Este documento serve como um roteiro leve para implementar o **AstroFetch** com ajuda de IA.

O AstroFetch não é um produto empresarial nem um jogo. É um app pessoal de terminal, escrito em Rust, inspirado no estilo do `screenFetch`: ele mostra informações do sistema ao lado de uma imagem ASCII astrofísica gerada de forma procedural.

A prioridade é manter o projeto simples, bonito, rápido, hackeável, multiplataforma e adequado para portfólio.

## Plataformas-Alvo

O AstroFetch deve funcionar em:

- Linux;
- macOS;
- Windows.

A experiência principal deve ser multiplataforma. Recursos específicos de sistema operacional devem ser opcionais e best-effort.

### Política de compatibilidade

- O comando padrão `astrofetch` deve funcionar em Linux, macOS e Windows.
- Nenhum recurso opcional pode quebrar o app quando não estiver disponível.
- Informações ausentes devem ser omitidas ou exibidas como `N/A`.
- Recursos Linux específicos, como `journalctl`, devem ser tratados como extensões opcionais.
- O projeto deve evitar assumir GNOME, KDE, X11, Wayland, systemd, Homebrew, Chocolatey, PowerShell ou qualquer gerenciador de pacotes específico no caminho principal.

## Visão do Projeto

O objetivo é produzir uma saída no terminal no estilo:

```text
[arte ASCII astrofísica]    user@host
[galáxia ou cluster]        OS: Ubuntu 24.04
[procedural]                Kernel: Linux 6.x
                             Uptime: 2h 34m
                             Shell: bash
                             CPU: AMD Ryzen ...
                             RAM: ...
                             Disk: ...
```

A imagem à esquerda deve substituir o logo fixo tradicional de ferramentas como `screenFetch`, usando uma estética inspirada em galáxias, campos estelares e estruturas astrofísicas.

## Princípios

- Começar por uma fatia vertical funcional.
- Preferir stdout simples antes de qualquer TUI complexa.
- Separar geração visual, coleta de sistema, terminal e layout.
- Evitar panics em uso normal.
- Fazer fallback gracioso quando alguma informação não estiver disponível.
- Manter dependências em número razoável.
- Priorizar uma saída bonita no terminal antes de sofisticação física.
- Tratar Linux, macOS e Windows como plataformas de primeira classe.
- Manter campos lentos fora do caminho padrão.

## Restrições Técnicas Críticas

### 1. Orçamento de performance

O comando padrão `astrofetch` deve ser rápido o suficiente para uso interativo no shell.

Regras:

- Não chamar subprocessos lentos no caminho padrão.
- Não executar `journalctl`, contagem de pacotes, detecção detalhada de GPU ou detecção de temas por padrão.
- Medir cold start com um comando simples de benchmark.
- Campos lentos devem ser opcionais, ter timeout curto e limite de saída.
- Quando possível, paralelizar coleta lenta com geração visual.

Meta inicial:

```text
astrofetch padrão: idealmente abaixo de 100 ms em máquina comum.
```

Essa meta não é uma garantia rígida, mas deve orientar decisões de implementação.

### 2. Layout robusto com ANSI e Unicode

Nunca calcule alinhamento com `String::len()`.

Motivos:

- Códigos ANSI ocupam bytes, mas não colunas visuais.
- Caracteres Unicode podem ocupar largura visual diferente.
- Emojis e alguns símbolos podem quebrar alinhamento dependendo do terminal.

Regras:

- Criar uma função `visible_width()`.
- Remover/ignorar ANSI ao calcular largura visual.
- Usar `unicode-width` ou solução equivalente.
- Usar ASCII puro como paleta padrão.
- Tratar paletas Unicode como opcionais.

### 3. Detecção de TTY e cor

Cores ANSI devem ser desativadas quando:

- `--no-color` for usado;
- a variável `NO_COLOR` estiver definida;
- stdout não for um TTY;
- o terminal não aparentar suportar cor.

### 4. Renderização em baixa resolução

A imagem final normalmente terá algo como 40x20 ou 50x25 caracteres. Isso é baixa resolução.

Regras:

- Não confiar apenas em fórmulas físicas ingênuas.
- Corrigir aspect ratio dos caracteres do terminal.
- Considerar supersampling/binning.
- Aplicar normalização robusta.
- Aplicar contraste visual antes do mapeamento ASCII.
- Testar gamma, log stretch ou asinh stretch.
- Garantir que a arte seja bonita mesmo quando a física for simplificada.

### 5. Subprocessos seguros

Quando for necessário chamar comandos externos:

- Usar `std::process::Command` com argumentos separados.
- Nunca montar comando via shell string.
- Aplicar timeout.
- Limitar bytes de stdout/stderr.
- Tratar erro de permissão e comando ausente.
- Não bloquear o modo padrão.

### 6. Watch mode sem flicker

O modo `watch` não deve usar apenas `clear` + `print!` em loop.

Regras:

- Usar controle de cursor.
- Renderizar frame em buffer.
- Atualizar a tela de forma previsível.
- Considerar `crossterm` quando o modo watch for implementado.
- Encerrar com Ctrl-C de forma segura.

## Fase 1: Setup do Projeto

Crie o esqueleto Rust usando Cargo.

Dependências iniciais sugeridas:

- `clap` com feature `derive`, para parsing de argumentos.
- `rand`, para seeds e geração procedural.
- `sysinfo`, para informações básicas multiplataforma do sistema.
- `unicode-width`, para cálculo de largura visual.
- `is-terminal`, para detectar se stdout é TTY.

Adiar para fases futuras:

- `crossterm`, até o modo watch ou controle de cursor ser necessário.
- crates específicas de systemd, Windows Event Log ou macOS logs.

Comandos esperados:

```bash
cargo build
cargo run -- --help
cargo test
cargo fmt
cargo clippy -- -D warnings
```

Critério de aceite:

- O projeto compila.
- `astrofetch --help` funciona.
- A estrutura de módulos inicial existe.
- O projeto compila no sistema atual antes de qualquer otimização.

## Fase 2: CLI Básica

Implemente o módulo `cli.rs` com uma interface simples.

Argumentos iniciais:

- `--model`: modelo visual. Valores: `random`, `elliptical`, `spiral`, `cluster`, `starfield`.
- `--width`: largura da arte ASCII. Padrão: `40`.
- `--height`: altura da arte ASCII. Padrão: `20`.
- `--seed`: seed opcional para saída determinística.
- `--no-color`: desativa cores ANSI.
- `--logo-only`: imprime apenas a imagem ASCII.
- `--info-only`: imprime apenas informações do sistema.
- `--compact`: reduz a quantidade de campos exibidos.

Critério de aceite:

- Os argumentos aparecem em `--help`.
- Valores inválidos retornam erro claro.
- A execução padrão `astrofetch` funciona sem argumentos.

## Fase 3: Terminal e Layout Primitives

Antes de colorir ou sofisticar a arte, implemente primitivas corretas de terminal.

Módulo sugerido: `terminal.rs`.

Responsabilidades:

- detectar TTY;
- detectar `NO_COLOR`;
- definir se cores estão habilitadas;
- calcular largura visual com `visible_width()`;
- remover ou ignorar ANSI no cálculo de largura;
- manter paleta ASCII padrão.

Critério de aceite:

- Testes cobrem strings com ANSI.
- Testes cobrem strings ASCII simples.
- Paleta padrão não depende de Unicode.
- `--no-color` e `NO_COLOR` desativam cor.

## Fase 4: Vertical Slice Multiplataforma

Implemente uma primeira versão funcional que já pareça com um fetch tool.

Nesta fase, a arte ASCII pode ser simples ou temporária. O importante é integrar:

- imagem à esquerda;
- informações do sistema à direita;
- layout alinhado por linhas;
- saída em stdout;
- suporte básico a Linux, macOS e Windows.

Campos mínimos:

- `user@host`;
- `OS`;
- `Kernel` ou versão equivalente;
- `Uptime`;
- `Shell`, quando disponível;
- `CPU`, preferencialmente modelo ou identificação;
- `RAM`;
- `Disk`, se simples de obter.

Critério de aceite:

- `cargo run` imprime uma tela completa.
- A saída lembra o formato visual do `screenFetch`.
- O programa não falha se algum campo não puder ser coletado.
- Campos não suportados na plataforma atual aparecem como `N/A` ou são omitidos.

## Fase 5: Motor Astrofísico Procedural

Implemente o módulo `engine.rs`.

O motor deve gerar uma matriz 2D de `f64`, com valores normalizados entre `0.0` e `1.0`, representando intensidade luminosa.

Modelos planejados:

### Starfield

Campo estelar simples para fallback visual.

Deve ser implementado primeiro por ser simples e útil para validar layout.

### Elíptica

Baseada em um perfil de Sérsic simplificado:

```text
I(R) = I_e * exp(-b_n * ((R/R_e)^(1/n) - 1))
```

Parâmetros úteis:

- índice de Sérsic;
- raio efetivo;
- elipticidade;
- contraste;
- correção de aspect ratio.

### Cluster

Aglomerado estelar com sorteios Monte Carlo.

Parâmetros úteis:

- número de estrelas;
- concentração;
- dispersão radial;
- brilho máximo;
- smoothing/binning.

### Espiral

Modelo visual inspirado em disco exponencial com braços espirais.

Parâmetros úteis:

- número de braços;
- inclinação;
- ruído;
- abertura dos braços;
- contraste.

Critério de aceite:

- Cada modelo gera uma matriz válida.
- A mesma seed produz a mesma saída.
- Os modelos são independentes da renderização e da coleta de sistema.
- A saída não parece apenas um borrão em 40x20.

## Fase 6: Normalização e Renderização ASCII

Implemente o módulo `render.rs`.

Responsabilidades:

- converter matriz numérica em caracteres ASCII;
- aplicar paleta de densidade;
- aplicar cor ANSI opcional;
- respeitar `--no-color`;
- aplicar normalização e contraste antes do mapeamento.

Paleta padrão:

```text
" .:-=+*#%@"
```

Paletas opcionais:

```text
" .,:;irsXA253hMHGS#9B&@"
" ·•✦✧*#%@"
```

Stretch/contraste a avaliar:

- linear;
- gamma;
- log;
- asinh;
- clipping por percentis.

Critério de aceite:

- Valores baixos viram caracteres leves.
- Valores altos viram caracteres densos.
- A saída sem cor é limpa e copiável.
- A renderização não depende das informações do sistema.
- O núcleo das galáxias tem contraste visível contra o fundo.

## Fase 7: Coleta Multiplataforma de Sistema

Implemente o módulo `system.rs`.

Estratégia:

- Definir uma estrutura `SystemSnapshot`.
- Usar `sysinfo` para campos multiplataforma quando viável.
- Usar variáveis de ambiente para shell/user quando confiáveis.
- Criar providers específicos por plataforma apenas quando necessário.

Campos básicos:

- user e hostname;
- OS;
- kernel ou versão do sistema;
- uptime;
- shell;
- disco;
- CPU;
- RAM.

Campos avançados e best-effort:

- packages;
- resolution;
- desktop environment;
- window manager;
- tema GTK;
- tema de ícones;
- fonte;
- GPU.

Critério de aceite:

- Falhas de coleta viram `N/A` ou simplesmente ocultam o campo.
- A coleta padrão não chama subprocessos lentos.
- O app continua útil fora de GNOME, fora de Ubuntu, no macOS e no Windows.
- Os campos avançados não bloqueiam o comando padrão.

## Fase 8: Layout Final

Implemente o módulo `layout.rs`.

Responsabilidades:

- combinar linhas da arte ASCII com linhas de informação;
- alinhar texto à direita da imagem;
- preservar espaçamento legível;
- lidar com altura diferente entre imagem e campos;
- suportar modo compacto;
- usar `visible_width()`.

Layout padrão:

```text
<ascii line 1>    <label>: <value>
<ascii line 2>    <label>: <value>
<ascii line 3>    <label>: <value>
```

Critério de aceite:

- O layout funciona em terminais comuns.
- A arte e os campos não ficam colados.
- O resultado visual é agradável.
- Cores ANSI não quebram alinhamento.
- Paletas Unicode opcionais não quebram o layout de forma catastrófica.

## Fase 9: Customização

Adicione opções úteis sem transformar o projeto em algo complexo.

Opções planejadas:

- `--fields`: lista explícita de campos a mostrar.
- `--hide`: lista de campos a esconder.
- `--palette`: escolhe paleta ASCII.
- `--color`: escolhe estilo/cor principal.
- `--left-padding`: controla margem esquerda.
- `--gap`: controla espaço entre imagem e informações.

Critério de aceite:

- O uso padrão continua simples.
- Customizações são opcionais.
- A CLI permanece fácil de entender.

## Fase 10: Campos Lentos e Cache Opcional

Campos como packages, GPU detalhada, temas e resolução podem exigir subprocessos ou APIs específicas.

Regras:

- Esses campos não devem ser padrão no MVP.
- Devem ter timeout curto.
- Devem ter fallback amigável.
- Podem ser coletados em paralelo com a engine.
- Cache simples pode ser considerado para valores estáveis.

Exemplos de comandos por plataforma, todos opcionais:

Linux:

- `dpkg-query`;
- `pacman`;
- `rpm`;
- `flatpak`;
- `snap`;
- `lspci`;
- `gsettings`;
- `xrandr`.

macOS:

- `sw_vers`;
- `sysctl`;
- `system_profiler`;
- `pmset`.

Windows:

- PowerShell;
- `wmic`, se disponível;
- APIs/crates específicas no futuro.

Critério de aceite:

- O comando padrão continua rápido.
- Falha em subprocesso não quebra o app.
- Timeouts são testáveis.

## Fase 11: Logs Opcionais por Plataforma

Objetivo: exibir algumas linhas recentes de logs do sistema como contexto.

A primeira implementação deve ser Linux-only via `journalctl`, sempre opcional.

Argumentos sugeridos:

- `--journal`;
- `--journal-lines`;
- `--journal-unit`;
- `--journal-priority`.

Estratégia Linux inicial:

- chamar `journalctl` como processo externo;
- limitar número de linhas;
- aplicar timeout curto;
- tratar ausência de `journalctl`;
- tratar erro de permissão sem panic.

macOS e Windows:

- não implementar no MVP;
- avaliar depois suporte opcional a `log show` no macOS e Windows Event Log no Windows.

Critério de aceite:

- `astrofetch --journal` mostra logs recentes quando possível no Linux.
- Se `journalctl` não existir, o app informa ou ignora graciosamente.
- O app não trava esperando logs.
- Linux journal não afeta macOS ou Windows.

## Fase 12: Arte Reativa

Permita que a imagem ASCII seja influenciada pelo estado do sistema.

Exemplos:

- CPU alta aumenta brilho ou turbulência;
- RAM alta aumenta densidade;
- load average aumenta número de estrelas;
- eventos críticos no journal alteram contraste ou símbolos.

Critério de aceite:

- O modo reativo é opcional.
- `--seed` ainda permite reprodutibilidade quando desejado.
- A arte continua bonita, não apenas caótica.

## Fase 13: Modo Watch

Adicione um subcomando:

```bash
astrofetch watch
```

Opções:

- `--interval`;
- `--journal`;
- `--no-color`;
- `--compact`.

Estratégia inicial:

- usar controle de cursor;
- renderizar frames em buffer;
- evitar `clear` ingênuo em loop;
- considerar `crossterm`;
- encerrar com Ctrl-C de forma segura.

Critério de aceite:

- O painel atualiza sem flicker excessivo.
- O consumo de CPU permanece baixo.
- O modo normal `astrofetch` continua instantâneo.

## Fase 14: CI e Portabilidade

Adicionar validação mínima em:

- Linux;
- macOS;
- Windows.

Critérios:

- `cargo fmt --check`;
- `cargo clippy -- -D warnings`;
- `cargo test`;
- build em matrix multiplataforma.

Testes importantes:

- determinismo por seed;
- normalização/renderização;
- largura visual;
- layout com ANSI;
- `--no-color`;
- fallback de campos ausentes;
- providers de sistema mockados.

## Estrutura Inicial Sugerida

```text
src/
  main.rs
  cli.rs
  app.rs
  engine.rs
  render.rs
  layout.rs
  terminal.rs
  system.rs
  error.rs
```

Possível evolução posterior:

```text
src/
  system/
    mod.rs
    snapshot.rs
    linux.rs
    macos.rs
    windows.rs
    commands.rs
  render/
    mod.rs
    ascii.rs
    color.rs
    palette.rs
    stretch.rs
  astro/
    mod.rs
    canvas.rs
    model.rs
    elliptical.rs
    spiral.rs
    cluster.rs
    field.rs
```

Comece simples. Só extraia submódulos quando os arquivos ficarem grandes.

## Prompt de Continuidade para IA

Use este projeto como um app pessoal de terminal em Rust, inspirado no screenFetch.

Não trate o AstroFetch como jogo. Não transforme em produto empresarial. O objetivo é um app leve e bonito para portfólio, com informações do sistema à direita e arte ASCII astrofísica procedural à esquerda.

Prioridades:
1. Entregar uma fatia vertical funcional primeiro.
2. Manter arquitetura simples.
3. Suportar Linux, macOS e Windows.
4. Separar engine visual, coleta de sistema, terminal e layout.
5. Evitar panics.
6. Preferir stdout simples antes de TUI complexa.
7. Não chamar subprocessos lentos no caminho padrão.
8. Calcular largura visual corretamente, sem usar `String::len()` para layout.
9. Usar paleta ASCII pura por padrão.
10. Aplicar contraste e correção de aspecto na renderização.
11. Implementar incrementalmente, com testes onde fizer sentido.

Ao implementar, comece por `astrofetch` imprimindo uma tela screenFetch-like mínima e multiplataforma. Depois melhore o motor visual e adicione campos extras.
