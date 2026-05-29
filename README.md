# AstroFetch

**AstroFetch** é um utilitário de linha de comando (CLI) ultrarrápido, escrito em Rust, que gera arte ASCII procedural de estruturas astrofísicas (galáxias elípticas, espirais e aglomerados estelares) para uso em terminais. 

Diferente de geradores baseados em imagens ou padrões aleatórios, o AstroFetch fundamenta sua renderização em perfis físicos de densidade e brilho superficial, como modelos de King e perfis de Sérsic. Projetado com latência mínima em mente, ele é a ferramenta ideal para substituir os tradicionais logos de sistemas operacionais em scripts de fetch.

## Funcionalidades
* **Geração Procedural Científica:** Utiliza matemática aplicada para calcular a probabilidade de distribuição de massa.
* **Zero Dependências de Imagem:** 100% offline e computado em tempo real.
* **Performance Extrema:** Escrito em Rust, otimizado para inicialização invisível a olho nu.
* **Integração Perfeita:** Feito especificamente para atuar como logo generator ao lado de monitores de sistema.

## Física e Implementação
Para modelar o brilho superficial de galáxias, o programa avalia o perfil de Sérsic:
I(R) = I_e * exp( -b_n * [ (R/R_e)^(1/n) - 1 ] )

Modulando o índice n, a CLI transita dinamicamente entre distribuições de discos exponenciais (espirais) e núcleos concentrados (elípticas). Para aglomerados estelares, implementa modelos de distribuição isotrópica com sorteios de Monte Carlo.

## Instalação
cargo install --git https://github.com/<SEU_USUARIO>/astrofetch.git

## Uso Básico
astrofetch

# Forçar a geração de uma galáxia elíptica com índice de Sérsic 4
astrofetch --model elliptical --sersic 4

# Gerar um aglomerado estelar em uma matriz customizada
astrofetch --model cluster --width 50 --height 25

## Integração com Fastfetch
Adicione a seguinte linha ao seu ~/.bashrc ou ~/.zshrc:
fastfetch --logo-type file --logo <(astrofetch)
