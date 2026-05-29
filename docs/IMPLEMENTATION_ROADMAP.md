# AstroFetch - Roadmap de Implementação e Instruções para IA

Você é um assistente de inteligência artificial programando em Rust. Seu objetivo é implementar o **AstroFetch**, um gerador de arte ASCII astrofísica procedural via CLI.

## Fase 1: Setup do Projeto e Dependências
Crie o esqueleto usando cargo. 
* Utilize a crate clap (com a feature derive) para o parsing de argumentos.
* Utilize a crate rand para os sorteios de Monte Carlo.

## Fase 2: Módulo CLI (cli.rs)
Implemente as estruturas do clap. A interface deve aceitar:
* --model: Enum opcional (Random, Elliptical, Spiral, Cluster). Padrão: Random.
* --width: Inteiro opcional. Padrão: 40.
* --height: Inteiro opcional. Padrão: 20.
* --sersic: Float opcional. Padrão: 1.0 a 4.0.

## Fase 3: Motor Físico (engine.rs)
Crie as funções que gerarão uma matriz 2D de f64 representando a intensidade luminosa (0.0 a 1.0).
1. Perfil de Sérsic: Implemente a fórmula matemática I(R) = I_e * exp( -b_n * [ (R/R_e)^(1/n) - 1 ] )
2. Aglomerados (Monte Carlo): Utilize uma distribuição normal bivariada 2D para gerar probabilidades radiais e sorteie coordenadas.

## Fase 4: Módulo de Renderização (render.rs)
Implemente a tradução da matriz numérica para ASCII.
* Defina uma escala de densidade (ex: ' ', '.', ':', '-', '=', '+', '*', '#', '%', '@').
* Mapeie linearmente o f64 (de 0.0 a 1.0) para os índices do array.

## Fase 5: Integração Principal (main.rs)
Reúna os módulos, invoque o parser, passe os parâmetros para o motor e imprima a String final no stdout sem panics.
