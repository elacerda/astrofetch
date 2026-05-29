# AstroFetch - Implementation Roadmap

Este documento serve como um roteiro leve para implementar o **AstroFetch** com ajuda de IA.

O AstroFetch não é um produto empresarial nem um jogo. É um app pessoal de terminal, escrito em Rust, inspirado no estilo do `screenFetch`: ele mostra informações do sistema ao lado de uma imagem ASCII astrofísica gerada de forma procedural.

A prioridade é manter o projeto simples, bonito, rápido, hackeável e adequado para portfólio.

## Visão do Projeto

O objetivo é produzir uma saída no terminal no estilo:

```text
[arte ASCII astrofísica]    user@host
[galáxia ou cluster]        OS: Ubuntu 24.04
[procedural]                Kernel: Linux 6.x
                             Uptime: 2h 34m
                             Shell: bash
                             Resolution: 3440x1440
                             DE: GNOME
                             WM: Mutter
                             CPU: AMD Ryzen ...
                             GPU: NVIDIA ...
                             RAM: ...
```

A imagem à esquerda deve substituir o logo fixo tradicional de ferramentas como `screenFetch`, usando uma estética inspirada em galáxias, campos estelares e estruturas astrofísicas.

## Princípios

- Começar por uma fatia vertical funcional.
- Preferir stdout simples antes de qualquer TUI complexa.
- Separar geração visual, coleta de sistema e layout.
- Evitar panics em uso normal.
- Fazer fallback gracioso quando alguma informação não estiver disponível.
- Manter dependências em número razoável.
- Priorizar uma saída bonita no terminal antes de sofisticação física.

## Fase 1: Setup do Projeto

Crie o esqueleto Rust usando Cargo.

Dependências iniciais sugeridas:

- `clap` com feature `derive`, para parsing de argumentos.
- `rand`, para seeds e geração procedural.
- `sysinfo`, para informações básicas do sistema, se a coleta manual via `/proc` não for desejada.

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

## Fase 3: Vertical Slice screenFetch-like

Implemente uma primeira versão funcional que já pareça com um fetch tool.

Nesta fase, a arte ASCII pode ser simples ou temporária. O importante é integrar:

- imagem à esquerda;
- informações do sistema à direita;
- layout alinhado por linhas;
- saída em stdout.

Campos mínimos:

- `user@host`
- `OS`
- `Kernel`
- `Uptime`
- `Shell`
- `CPU`
- `RAM`

Critério de aceite:

- `cargo run` imprime uma tela completa.
- A saída lembra o formato visual do `screenFetch`.
- O programa não falha se algum campo não puder ser coletado.

## Fase 4: Motor Astrofísico Procedural

Implemente o módulo `engine.rs`.

O motor deve gerar uma matriz 2D de `f64`, com valores normalizados entre `0.0` e `1.0`, representando intensidade luminosa.

Modelos planejados:

### Elíptica

Baseada em um perfil de Sérsic simplificado:

```text
I(R) = I_e * exp(-b_n * ((R/R_e)^(1/n) - 1))
```

Parâmetros úteis:

- índice de Sérsic;
- raio efetivo;
- elipticidade;
- contraste.

### Espiral

Modelo visual inspirado em disco exponencial com braços espirais.

Parâmetros úteis:

- número de braços;
- inclinação;
- ruído;
- abertura dos braços.

### Cluster

Aglomerado estelar com sorteios Monte Carlo.

Parâmetros úteis:

- número de estrelas;
- concentração;
- dispersão radial;
- brilho máximo.

### Starfield

Campo estelar simples para fallback visual.

Critério de aceite:

- Cada modelo gera uma matriz válida.
- A mesma seed produz a mesma saída.
- Os modelos são independentes da renderização e da coleta de sistema.

## Fase 5: Renderização ASCII

Implemente o módulo `render.rs`.

Responsabilidades:

- converter matriz numérica em caracteres ASCII;
- aplicar paleta de densidade;
- aplicar cor ANSI opcional;
- respeitar `--no-color`.

Paletas iniciais:

```text
" .:-=+*#%@"
" ·•✦✧*#%@"
" .,:;irsXA253hMHGS#9B&@"
```

Critério de aceite:

- Valores baixos viram caracteres leves.
- Valores altos viram caracteres densos.
- A saída sem cor é limpa e copiável.
- A renderização não depende das informações do sistema.

## Fase 6: Coleta de Informações do Sistema

Implemente o módulo `system.rs`.

Campos inspirados no uso atual com `screenFetch`:

- user e hostname;
- OS;
- kernel;
- uptime;
- packages, se simples de obter;
- shell;
- resolution;
- desktop environment;
- window manager;
- tema GTK, se simples de obter;
- tema de ícones, se simples de obter;
- fonte, se simples de obter;
- disco;
- CPU;
- GPU;
- RAM.

Não é necessário implementar todos de uma vez. Comece pelos campos confiáveis e adicione os demais incrementalmente.

Critério de aceite:

- Falhas de coleta viram `N/A` ou simplesmente ocultam o campo.
- A coleta não deve tornar o app perceptivelmente lento.
- O app continua útil mesmo fora de GNOME ou fora de Ubuntu.

## Fase 7: Layout

Implemente o módulo `layout.rs`.

Responsabilidades:

- combinar linhas da arte ASCII com linhas de informação;
- alinhar texto à direita da imagem;
- preservar espaçamento legível;
- lidar com altura diferente entre imagem e campos;
- suportar modo compacto.

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

## Fase 8: Customização

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

## Fase 9: Journal Opcional

Implemente o módulo `journal.rs`.

Objetivo: exibir algumas linhas recentes do `journalctl` como contexto do sistema.

Argumentos sugeridos:

- `--journal`
- `--journal-lines`
- `--journal-unit`
- `--journal-priority`

Estratégia inicial:

- chamar `journalctl` como processo externo;
- limitar número de linhas;
- aplicar timeout curto;
- tratar ausência de `journalctl`;
- tratar erro de permissão sem panic.

Critério de aceite:

- `astrofetch --journal` mostra logs recentes quando possível.
- Se `journalctl` não existir, o app informa ou ignora graciosamente.
- O app não trava esperando logs.

## Fase 10: Arte Reativa

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

## Fase 11: Modo Watch

Adicione um subcomando:

```bash
astrofetch watch
```

Opções:

- `--interval`
- `--journal`
- `--no-color`
- `--compact`

Estratégia inicial:

- limpar/redesenhar a tela periodicamente;
- evitar TUI completa no começo;
- encerrar com Ctrl-C de forma segura.

Critério de aceite:

- O painel atualiza sem flicker excessivo.
- O consumo de CPU permanece baixo.
- O modo normal `astrofetch` continua instantâneo.

## Estrutura Inicial Sugerida

```text
src/
  main.rs
  cli.rs
  app.rs
  engine.rs
  render.rs
  system.rs
  layout.rs
  journal.rs
  error.rs
```

Responsabilidades:

- `main.rs`: entrada do binário.
- `cli.rs`: argumentos e subcomandos.
- `app.rs`: orquestração do fluxo.
- `engine.rs`: geração procedural astrofísica.
- `render.rs`: conversão para ASCII.
- `system.rs`: snapshot do sistema.
- `layout.rs`: composição visual.
- `journal.rs`: leitura opcional do journal.
- `error.rs`: erros recuperáveis.

## Prompt de Continuidade para IA

Use este projeto como um app pessoal de terminal em Rust, inspirado no screenFetch.

Não trate o AstroFetch como jogo. Não transforme em produto empresarial. O objetivo é um app leve e bonito para portfólio, com informações do sistema à direita e arte ASCII astrofísica procedural à esquerda.

Prioridades:
1. Entregar uma fatia vertical funcional primeiro.
2. Manter arquitetura simples.
3. Separar engine visual, coleta de sistema e layout.
4. Evitar panics.
5. Preferir stdout simples antes de TUI complexa.
6. Implementar incrementalmente, com testes onde fizer sentido.

Ao implementar, comece por `astrofetch` imprimindo uma tela screenFetch-like mínima. Depois melhore o motor visual e adicione campos extras.
