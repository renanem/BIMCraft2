# Continuar no outro PC (cole este arquivo inteiro no Claude Code, na pasta do repositório)

Você continua um trabalho de contribuição ao BIMCraft (upstream `IPedrax/BIMCraft`, fork `renanem/BIMCraft2`). Leia o `CLAUDE.md` do repositório antes de tudo: as regras
valem aqui (sem `unwrap`/`panic` fora de testes, testes primeiro, `cargo fmt`, clippy com `-D warnings`, limite de 3000 palavras no `CLAUDE.md`).

**Atribuição:** tudo deve aparecer como feito por Renan Escoqui Meyer. Sem `Co-Authored-By`, sem rodapé de ferramenta, texto de PR e issue em primeira pessoa.

## Estado ao sair

Branch `feat/fire-checks` (esta), baseada em `feat/reservoir-size` (PR #261, `reservoir_size`). Contém, **ainda sem PR**:

- `crates/core/src/checks.rs`: `Kind`, `Status`, `Check` e ajudantes (`to_json`, `pending`, `none_failed`), comuns a todos os comandos abaixo. `reservoir.rs` já usa.
- `extinguisher.rs` + comando `extinguisher_size`: IT 21/2025 (CBPMESP).
- `emergency.rs` + comando `emergency_light`: IT 18/2025 e NBR 10898 (autonomia 2 h, relação 20:1, Tabela A.1).

Na última rodada de testes: 33 passaram e 1 falhou (o texto da cozinha profissional); a correção foi feita e **não foi reexecutada**. Confirme primeiro:

```sh
cargo fmt --check
CARGO_INCREMENTAL=0 cargo test -p bimcraft-core -- reservoir extinguisher emergency
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

No Windows, o linker já falhou com "símbolo externo não resolvido" no build incremental: use `CARGO_INCREMENTAL=0` ou `cargo clean -p bimcraft-core`.

Depois: `cargo i18n update`, e documentar os dois comandos em `docs/areas/structure-and-mep.md` (um parágrafo cada) e numa linha de módulo em `CLAUDE.md` (cuidado com o limite de palavras: encurte outra linha em vez de estourar).

## Regras sobre PR

**Não abra PR de incêndio no upstream** até Renan avisar que já falou com o mantenedor (IPedrax). A issue #260 trata da base hidráulica (biblioteca de aparelhos, consumo, quatro categorias de resultado); espere a resposta dela antes de mexer em `water.rs` ou `drainage.rs`.

## Falta fazer

### 1. `sprinkler_check` (IT 23/2025 + NBR 10897:2020), módulo `sprinkler.rs`

Só funções puras e um comando, sem mexer na interface. Resultado no formato de `checks.rs`. Dados já lidos, citar a tabela e o item:

- Risco: leve (até 300 MJ/m²), ordinário 1 (300 a 800), ordinário 2 (800 a 1200), extra 1 (acima de 1200, sem líquidos), extra 2 (M-2 ou líquidos). D-1 e D-2 são leve; G-1 e G-2 são ordinário 2 (IT 23 item 2.4).
- Cobertura máxima e distância (IT 23 Tabelas 2 a 6 = NBR 10897 Tabelas 10 a 14): padrão (por tabela e por cálculo hidráulico, por tipo de teto), estendida, lateral (por acabamento), CCAE e ESFR. `As = S x L`, com S e L pelo maior entre o dobro da distância à parede e a distância ao próximo.
- Parede: no máximo metade da distância máxima entre chuveiros, no mínimo 100 mm; entre chuveiros 1,8 m (padrão) ou 2,4 m (estendida, CCAE, ESFR); 3/4 em paredes em ângulo.
- Tubulação por tabela (NBR 10897 Tabelas 31, 33, 34 e 35): leve e ordinário, aço e cobre, número de chuveiros por DN; só para sistema novo com área abaixo de 465 m², K 80, e tubo de aço inoxidável ou cobre; extra sempre por cálculo hidráulico. Mínimo DN 25 (ferroso) ou DN 20 (cobre e não metálicos).
- Área máxima por coluna e pavimento (Tabela 8): leve e ordinário 4800 m², extra e armazenamento 3700 m², tabelas 2300 m².
- Demanda de água: curvas densidade x área (NBR 10897 Figura 42), pontos nos fins das curvas lidos da figura; área mínima 140 m² (leve e ordinário) e 230 m² (extra); ajustes de +30% (teto inclinado acima de 16,7% e ação prévia de duplo bloqueio) e −25% até 190 m² (temperatura alta em extra). **Vários ajustes ao mesmo tempo ficam `pending`** (o item 8.5.5.7 diz "cumulativos"; a interpretação é do projetista).
- Tetos acima de 6,1 m: Tabelas 23 a 26 (K mínimo e demanda).
- Hidrantes somados: NBR 10897 Tabela 29 (380, 950 e 1900 L/min; 30, 60 e 90 min) e `hydrant.rs` já existente.
- Reserva: duração Tabela 27 (30, 60 e 90 min); por tabela, Tabela 28.
- Perda de carga: Hazen-Williams `J = 605 · Q^1,85 / (C^1,85 · d^4,87) · 10^5` (kPa/m, Q em L/min, d em mm), C da Tabela 30 (aço preto molhado 120, seco 100, galvanizado 120, plástico 150, ferro dúctil com cimento 140, cobre e inox 150); pressão de velocidade `255 Q² / D⁴`; `K = Q/√p`; pressão de 48 kPa a 1210 kPa (extra); tolerância de 1 kPa no balanceamento.
- Espessuras de parede do aço (Tabela 3) e diâmetro interno: use `d = DN - 2 x espessura` só se a Tabela 3 for citada; senão deixe o diâmetro como entrada.
- Estoque de sobressalentes (item 5.2.7): 6, 12 ou 24; no mínimo 4 de cada tipo, modelo e temperatura.
- Comprimentos equivalentes de conexões: a NBR 10897 manda buscar com o fabricante ou literatura. **Não invente**: entrada do projeto, ou a tabela da Tigre que `water.rs` já cita se Renan enviar a página.

### 2. `hot_water_size` (NBR 5626:2020), módulo `hot_water.rs`

A NBR 5626:2020 substituiu a NBR 7198 (a "ISO 7198:2021" que veio numa busca é de próteses vasculares: ignore). Itens a citar: vazão `Q = K · √P` (6.9.2), velocidade 3 m/s (nota de 6.8.3), pressão dinâmica mínima de 10 kPa no ponto e 5 kPa em qualquer ponto (6.9.2 a 6.9.4), estática máxima de 400 kPa (6.9.5), sobrepressão de transiente até 200 kPa (6.9.7), 70 °C em tubulação dentro de ambiente sanitário com misturador (6.10.3), 45 °C onde há uso corporal (6.10.4.3), isolamento em toda a extensão (6.12.3), válvula de alívio nos aquecedores de acumulação (6.13.3), perda de carga pela equação universal (6.14.4: `water.rs` já tem Darcy-Weisbach e Colebrook; reaproveite, sem duplicar). Aparelhos e vazões de projeto são do projetista e do fabricante (6.7.1 e 6.14.1): entrada, nunca valor da norma. Material da tubulação como dado (CPVC, PPR, cobre).

### 3. `reservoir_size` (já no PR #261)

Trocar a fonte "as read from its text" pelos itens: 6.5.6.2 (24 h e incêndio conjunto), 6.5.6.3 e sua nota (três dias), 6.5.6.5 (dois compartimentos e o menor para o pico de manutenção), 6.7.2 (6 h e 3 h), 6.5.11.2 (duas bombas), 6.5.11.14 (reserva para duas ou mais torres). A fórmula de Forchheimer **não** está na norma: só entra como método opcional, marcado "prática de engenharia".

### 4. Pendências por tema

- IFC de MEP (tubos, dutos, eletrodutos e cabos já saem com classe própria; falta só a classe de chuveiros e extintores, que depende da issue #260 e do mantenedor).
- NBR 10898: autonomia e uniformidade já entraram; falta o tempo de comutação do gerador e os limites de tensão, que o IT 18 remete à norma.
- IT 21: o enquadramento baixo, médio e alto vem da IT 14 (carga de incêndio); hoje é entrada `risk`.

## Ao terminar

Rodar os quatro comandos de verificação, commitar como Renan, empurrar para `origin` e **só então** perguntar ao Renan se pode abrir os PRs.
