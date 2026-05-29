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
                              CPU: AMD Ryzen ...
                              RAM: ...
                              Disk: ...
```

No futuro, o AstroFetch também poderá mostrar informações opcionais mais ricas, como GPU, pacotes instalados, desktop environment, window manager, temas gráficos e um pequeno resumo de logs do sistema.

## Plataformas-alvo

O AstroFetch deve funcionar em:

- Linux;
- macOS;
- Windows.

A experiência principal deve ser multiplataforma. Campos específicos de cada sistema operacional devem ser best-effort: quando uma informação não estiver disponível, o app deve omitir o campo ou mostrar um fallback simples, sem falhar.

## Funcionalidades planejadas

- Arte ASCII astrofísica procedural.
- Modelos visuais como galáxia elíptica, galáxia espiral, aglomerado estelar e campo de estrelas.
- Informações básicas do sistema no estilo `screenFetch`.
- Layout com arte à esquerda e informações à direita.
- Cores ANSI opcionais.
- Modo sem cor para logs, prints, redirecionamento e compatibilidade.
- Seeds determinísticas para reproduzir a mesma imagem.
- Integrações opcionais por plataforma, como `journalctl` no Linux.
- Modo dinâmico futuro, com atualização periódica.

## Roadmap curto

1. **Vertical slice multiplataforma:** imprimir uma arte ASCII simples à esquerda e informações básicas do sistema à direita em Linux, macOS e Windows.
2. **Layout robusto:** calcular largura visual corretamente, sem quebrar com ANSI ou caracteres Unicode.
3. **Motor astrofísico procedural:** gerar galáxias, clusters e campos estelares com normalização, contraste e correção de aspecto para terminal.
4. **Campos de sistema confiáveis:** adicionar OS, kernel, uptime, shell, CPU, RAM e disco usando uma camada multiplataforma.
5. **Campos avançados opcionais:** adicionar GPU, pacotes, resolução, DE/WM e temas como campos best-effort e nunca bloqueantes.
6. **Customização:** adicionar opções como `--no-color`, `--logo-only`, `--info-only`, `--model`, `--seed`, `--width`, `--height`, `--palette`, `--fields` e `--hide`.
7. **Logs opcionais:** exibir eventos recentes quando houver suporte na plataforma, começando por `journalctl` no Linux.
8. **Arte reativa:** permitir que a imagem ASCII varie de acordo com CPU, RAM, load average ou eventos recentes.
9. **Modo watch:** atualizar a tela periodicamente sem flicker excessivo.

## Uso pretendido

```bash
astrofetch
```

```bash
astrofetch --model spiral --width 40 --height 20
```

```bash
astrofetch --seed 42 --no-color
```

```bash
astrofetch --logo-only
```

```bash
astrofetch --fields os,kernel,uptime,shell,cpu,ram,disk
```

```bash
astrofetch --journal --journal-lines 5
```

```bash
astrofetch watch --interval 2s
```

## Restrições técnicas importantes

O comando padrão deve ser rápido. Campos que exigem subprocessos lentos, como contagem de pacotes, GPU detalhada, temas de desktop e logs, não devem bloquear o caminho principal.

O layout deve usar largura visual de terminal, não `String::len()`, porque códigos ANSI não ocupam colunas visuais e caracteres Unicode podem ter largura diferente de um byte.

A paleta padrão deve ser ASCII puro para máxima compatibilidade. Paletas Unicode podem ser opcionais.

A renderização astrofísica deve ser tratada como visualização de baixa resolução. Antes de mapear valores para caracteres ASCII, o app deve aplicar normalização, contraste e correção de aspecto.

Cores ANSI devem ser desativadas quando `--no-color` for usado, quando a saída não for um TTY ou quando a variável `NO_COLOR` estiver definida.

Subprocessos devem ser opcionais, ter timeout curto, limite de saída e ser executados sem shell interpolation.

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
  app.rs
  engine.rs
  render.rs
  layout.rs
  terminal.rs
  system.rs
  error.rs
```

- `cli.rs`: parsing de argumentos com `clap`.
- `app.rs`: orquestra o fluxo principal.
- `engine.rs`: geração procedural da matriz de luminosidade.
- `render.rs`: conversão da matriz numérica para ASCII.
- `layout.rs`: composição da arte com os dados textuais.
- `terminal.rs`: largura visual, ANSI, TTY, cores e controle de terminal.
- `system.rs`: coleta multiplataforma de informações do sistema.
- `error.rs`: erros recuperáveis.

Módulos específicos de plataforma podem surgir depois, por exemplo:

```text
src/system/
  linux.rs
  macos.rs
  windows.rs
```

## Filosofia do projeto

Este não é um produto empresarial nem uma tentativa de substituir ferramentas maduras como `fastfetch` ou `screenFetch`.

O AstroFetch é um projeto pessoal, divertido e visual, criado para explorar Rust, terminal UI simples, arte ASCII procedural, coleta multiplataforma de sistema e um pouco de estética astrofísica no terminal.

O objetivo é que ele seja leve, bonito, hackeável e agradável de rodar no próprio shell.
