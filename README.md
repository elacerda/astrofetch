# AstroFetch

**AstroFetch** é um app pessoal de terminal, escrito em Rust, inspirado no estilo do `screenFetch`: ele mostra informações do sistema ao lado de uma imagem ASCII astrofísica gerada de forma procedural.

A ideia é simples e despojada: trocar o logo fixo da distribuição por uma galáxia, aglomerado estelar ou campo de estrelas em ASCII, mantendo o charme dos antigos fetch tools de terminal.

> Status: projeto experimental, feito para uso pessoal e portfólio.

## Ideia

O objetivo inicial é produzir uma saída parecida com isto:

```text
[galáxia ASCII procedural]    user@host
[ou aglomerado estelar]       OS: Ubuntu 24.04
[ou campo de estrelas]        Kernel: Linux 6.x
                              Uptime: 2h 34m
                              Shell: bash
                              Resolution: 3440x1440
                              DE: GNOME
                              WM: Mutter
                              CPU: AMD Ryzen ...
                              GPU: NVIDIA ...
                              RAM: ...
```

No futuro, o AstroFetch também poderá mostrar um pequeno resumo do journal do sistema, criando uma espécie de fetch visual com contexto recente da máquina.

## Funcionalidades planejadas

- Arte ASCII astrofísica procedural.
- Modelos visuais como galáxia elíptica, galáxia espiral, aglomerado estelar e campo de estrelas.
- Informações básicas do sistema no estilo `screenFetch`.
- Layout com arte à esquerda e informações à direita.
- Cores ANSI opcionais.
- Modo sem cor para logs, prints e compatibilidade.
- Seeds determinísticas para reproduzir a mesma imagem.
- Integração opcional com `journalctl`.
- Modo dinâmico futuro, com atualização periódica.

## Roadmap curto

1. **Vertical slice screenFetch-like:** imprimir uma arte ASCII simples à esquerda e informações básicas do sistema à direita.
2. **Motor astrofísico procedural:** gerar galáxias, clusters e campos estelares a partir de parâmetros simples.
3. **Campos completos de sistema:** adicionar OS, kernel, uptime, shell, resolução, desktop environment, window manager, CPU, GPU, RAM e disco.
4. **Customização:** adicionar opções como `--no-color`, `--logo-only`, `--info-only`, `--model`, `--seed`, `--width`, `--height` e `--palette`.
5. **Journal opcional:** exibir linhas recentes do `journalctl` com limite, timeout e tratamento seguro de erros.
6. **Arte reativa:** permitir que a imagem ASCII varie de acordo com CPU, RAM, load average ou eventos recentes.
7. **Modo watch:** atualizar a tela periodicamente como um mini-dashboard de terminal.

## Uso pretendido

```bash
astrofetch
```

```bash
astrofetch --model spiral --width 40 --height 20
```

```bash
astrofetch --no-color
```

```bash
astrofetch --logo-only
```

```bash
astrofetch --journal --journal-lines 5
```

```bash
astrofetch watch --interval 2s
```

## Desenvolvimento

```bash
cargo build
cargo run -- --help
cargo test
cargo fmt
cargo clippy -- -D warnings
```

## Arquitetura inicial

```text
src/
  main.rs
  cli.rs
  engine.rs
  render.rs
  system.rs
  layout.rs
  journal.rs
```

- `cli.rs`: parsing de argumentos com `clap`.
- `engine.rs`: geração procedural da matriz de luminosidade.
- `render.rs`: conversão da matriz numérica para ASCII.
- `system.rs`: coleta de informações do sistema.
- `layout.rs`: composição da arte com os dados textuais.
- `journal.rs`: leitura opcional de eventos recentes via `journalctl`.

## Filosofia do projeto

Este não é um produto empresarial nem uma tentativa de substituir ferramentas maduras como `fastfetch` ou `screenFetch`.

O AstroFetch é um projeto pessoal, divertido e visual, criado para explorar Rust, terminal UI simples, arte ASCII procedural e um pouco de estética astrofísica no terminal.

O objetivo é que ele seja leve, bonito, hackeável e agradável de rodar no próprio shell.
