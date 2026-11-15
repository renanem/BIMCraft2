# Rodada de conferência no Revit 2027

O que já rodou dentro do Revit está em [README.md](README.md#verificação). Esta rodada cobre o que ainda não rodou: a
exportação completa de um projeto, a leitura das configurações de um template e o botão em si. Nada aqui salva um arquivo
do Revit: cada documento é aberto em memória, lido e fechado sem salvar.

## O que precisa estar pronto

- Revit 2027 fechado (o `pyrevit run` abre uma instância própria e a fecha).
- pyRevit ligado ao 2027: `pyrevit attached` deve listar `Product: "2027"`.
- `bimcraft-cli` compilado desta branch: `cargo build -p bimcraft-cli`.

## 1. Exportar, dentro do Revit, sem diálogos

Escreva o que exportar em `%TEMP%\bimcraft-bridge-job.json`:

```json
{
  "tools": "C:\\Dev\\Duall\\BIMCraft2-revit\\tools\\revit_bridge",
  "documents": [
    "C:\\Users\\Meyer\\Documents\\Trabalho - Projetos\\Template Hidráulica Duall-2027 -NOVO limpo.rte",
    "C:\\Users\\Meyer\\Downloads\\Elétrica_selected_2026-08-18_03-19-51pm\\ELE-TIPO1.rvt",
    "C:\\ProgramData\\Autodesk\\RVT 2027\\Libraries\\English\\US\\Structural Precast\\Mounting Parts\\Solid Wall - Lifter.rfa"
  ],
  "out": "C:\\Temp\\bimcraft-conferencia",
  "limit": null
}
```

- `tools`: a pasta onde estão `script.py` e `bridge.py`.
- `documents`: templates (`.rte`), projetos (`.rvt`) e famílias (`.rfa`), em qualquer mistura. Um arquivo de versão
  anterior é convertido pelo Revit na abertura, em memória, o que demora.
- `limit`: quantas famílias carregadas ler de cada projeto; `null` lê todas (cada uma é aberta com `EditFamily`, cerca de
  um a dois segundos por família; uma de muitos tipos e geometria pesada, minutos).
- `hooks`: `true` mantém ligados os hooks de evento do pyRevit; sem isso o script os desliga na sessão que ele abre.

Depois:

```powershell
pyrevit run "C:\Dev\Duall\BIMCraft2-revit\tools\revit_bridge\headless.py" --revit=2027
```

O andamento fica em `out\bridge.log`, linha a linha com o tempo. Se o Revit parar no meio, a última linha diz onde. Um
erro antes de haver pasta de saída fica em `bimcraft-bridge-job.json.error.txt`, ao lado do arquivo de tarefa. O Revit dá
a cada sessão uma pasta temporária própria (`%TEMP%\<guid>`): o script procura a tarefa nela e na pasta acima, que é o
`%TEMP%` do usuário. O Revit leva cerca de dois minutos para subir antes da primeira linha do log.

## 2. Conferir no BIMCraft

```powershell
python tools\revit_bridge\check_export.py "C:\Temp\bimcraft-conferencia" --cli target\debug\bimcraft-cli.exe
```

Para cada pacote: abre o `.rte.json` como projeto, salva como `.bimt`, cria um projeto novo a partir desse `.bimt`, e, num
pacote de uma família só, carrega a família e coloca um componente. Imprime o que entrou e cada nota do que ficou de fora, e
guarda tudo em `check.json`.

## 3. O que olhar

| Parte | O que conferir | Onde |
|---|---|---|
| Famílias MEP do projeto | categoria certa, tipos, parâmetros de instância e de tipo, fórmulas mantidas e recusadas | `families/*.rfa.json`, notas de cada `Family …` |
| Geometria | a caixa da malha de um tipo bate com suas dimensões; nenhum sólido hospedeiro | abrir uma família no BIMCraft e colocar |
| Níveis e fases | mesmos nomes, elevações e ordem do Revit | `came_in` e a lista do projeto |
| Padrões de linha | segmentos em mm; os recusados por limite (0,05 a 100 mm, até 32 segmentos) | notas `line patterns left out` |
| Padrões de preenchimento | ângulo, espaçamento, deslocamento, traços; desenho e modelo | Manage > Fill Patterns |
| Estilos de linha | caneta, cor, padrão | Manage > Line Styles |
| Tipos de texto e de cota | fonte, tamanho em mm no papel, estilo | Manage |
| Estilos de objeto | caneta de projeção e de corte por categoria | Manage > Object Styles |
| Parâmetros de projeto | GUID dos compartilhados, instância/tipo, categorias MEP | Manage > Project Parameters; notas `Parameter …` |
| Filtros | categorias e regras; os deixados de fora por OR ou por unidade interna | notas `view filters left out` |
| Modelos de vista | tipo de vista, escala, detalhe, estilo, categorias ocultas, filtros aplicados, o que controla | View > View Templates |
| Carimbos | linhas e textos no lugar; rótulos não vêm | Manage > Title Blocks |
| Folhas | número, nome, tamanho, carimbo | navegador |

## 4. O botão em si

Com o pyRevit carregado no Revit 2027, copie `script.py` e `bridge.py` para a pasta do botão (README), abra um projeto e
clique em **BIMCraft → Exchange → Export**. Conferir: o diálogo de pasta, a recusa quando `BIMCraft-exchange` já existe, a
mensagem final com a contagem, e que o pacote gravado é igual ao do passo 1 para o mesmo arquivo.

## O que a rodada de 2026-11-15 mostrou

Rodou no Revit 2027 (27.0.4, pyRevit 6.4, IronPython 2.7.12) sobre o template de Hidráulica (263 MB, 558 famílias) e o
`ELE-TIPO1.rvt` (194 MB), por `headless.py`, e o resultado foi carregado no BIMCraft por `check_export.py`.

Defeitos achados e corrigidos:

- `headless.py` não achava o arquivo de tarefa: o Revit dá a cada sessão um `%TEMP%\<guid>` próprio.
- Nomes com acento eram recusados ao gravar (o `json` do IronPython 2 no modo ASCII): perdiam-se filtros, parâmetros,
  preenchimentos, modelos de vista e carimbos.
- O nome de tipos de texto e de cota não era lido (`Element.Name` escondido na subclasse), e o parâmetro da extensão da
  linha de chamada tinha o nome errado (`WITNS_LINE_EXTENSION`).
- Uma família sem tipo nomeado (tipo de nome em branco) era recusada: o tipo recebe o nome da família, como o Revit faz.
- O passo negativo de um padrão de preenchimento era recusado: sai como passo positivo com o deslocamento invertido.
- Regras de filtro sobre parâmetros internos vinham só com o nome traduzido: agora levam o nome público do
  `BuiltInParameter`, e o BIMCraft liga tipo de sistema, diâmetro, comprimento, nome da família e do tipo aos seus campos.
- Modelos de vista com mais de 32 filtros eram recusados inteiros: ficam os 32, os que ocultam primeiro.
- Texto de várias linhas num carimbo era recusado: vira um texto por linha.
- Centenas de notas iguais (padrões de linha de DWG sem segmentos) viram uma nota só.

O que entrou, depois das correções:

| | Template de Hidráulica | ELE-TIPO1 |
|---|---|---|
| Padrões de linha | 113 de 141 | 264 de 927 |
| Estilos de linha | 51 de 51 | 137 de 137 |
| Padrões de preenchimento | 214 de 238 | 122 de 125 |
| Tipos de texto | 20 de 20 | 298 de 299 |
| Tipos de cota | 43 de 43 | 31 de 31 |
| Estilos de objeto | 36 de 137 | 36 de 157 |
| Parâmetros de projeto | 169 de 275 | 21 de 148 |
| Filtros de vista | 52 de 74 | 59 de 71 |
| Modelos de vista | 31 de 31 | 26 de 26 |
| Carimbos | 2 de 3 | 2 de 5 |
| Folhas | 1 de 1 | 2 de 2 |
| Famílias (limite pedido) | 45 de 45 | 20 de 20 |

Os números do template são da rodada 4, conferidos com o leitor já corrigido. O que ficou de fora e por quê:

- **Parâmetros de projeto**: os ligados só a folhas, vistas, níveis ou outras categorias sem parâmetros no BIMCraft
  (103 no template; no ELE-TIPO1, 104, mais 21 em conflito de nome com um parâmetro que o projeto já tem).
- **Filtros**: regras sobre Descrição, Fabricante, Comentários de tipo, Função e Tipo de serviço, que não são campos no
  BIMCraft; regras unidas por OU; um filtro cuja regra aponta para um parâmetro que não existe mais.
- **Filtros por sistema**: entram para os tubos; conexões, acessórios e peças não têm o campo `System`, e a nota diz.
- **Carimbos** com mais de 1189 mm de largura (os de 1450 mm).
- **Estilos de objeto** de categorias sem linha própria no BIMCraft (101 e 121).
- **Padrões de preenchimento** vindos de DWG com origem a mais de 100 m da do padrão, e um com 397 famílias de linhas
  (o limite é 256).
- **Padrões de linha** importados de DWG sem segmentos, ou com segmento acima de 100 mm.

## O template inteiro, numa execução só

A primeira tentativa com as 558 famílias leu 63 em 23 minutos e parou. A causa não era a geometria: nesta instalação
estão ligados os *hooks* de desenvolvimento do pyRevit (`pyRevitDevHooks`), scripts que rodam a cada documento aberto,
alterado ou fechado, e a ponte abre um documento por família. Três mudanças:

- `headless.py` desliga os hooks do pyRevit **só na sessão do Revit que ele abre** (as configurações do pyRevit não são
  tocadas; `"hooks": true` na tarefa os mantém).
- A ponte grava cada família assim que a lê (`bridge.Bundle`), e o `.rte.json` e o `report.json` a cada família: uma
  execução interrompida deixa tudo o que leu, com `"complete": false` no relatório.
- A geometria de uma família tem orçamento: 100 mil triângulos e 90 segundos somados nos tipos; passou disso, os tipos
  restantes vêm só com os valores, e a nota diz quantos.

Com isso o template inteiro saiu do Revit 2027 em **43 minutos**: 550 famílias exportadas, 1 recusada (tem um nome em branco, de parâmetro ou da própria família; a ponte não diz qual), 7 não editáveis (montantes e painéis de sistema). Desses 43 minutos, 26 foram de três
famílias (`D - SEKAPISO SP80`, `SP46`, `SP30`: 13 tipos cada, até 80 mil triângulos), em que o próprio Revit demora a
regenerar cada tipo; as outras 547 levaram cerca de 17 minutos. O pacote tem 318 MB e o `.bimt` salvo, 311 MB.

No BIMCraft entraram as **550 famílias**, depois de três correções que só o pacote inteiro mostrou:

- **No núcleo** (`engine.rs`): o motor troca por nome todo GUID de um comando que seja de um item da biblioteca. O GUID
  `shared` de um parâmetro de família é o de um parâmetro de projeto quando o template tem os dois, e virava o nome: 311
  famílias recusadas. `shared` fica como GUID, como `guid` e `definition` já ficavam.
- **Nomes de parâmetros compartilhados**: o Revit aceita dois parâmetros compartilhados diferentes com o mesmo nome (o
  `Acabamento` da Amanco e o da Duall) e um de família com o nome de um parâmetro de projeto (`IfcDescription`); aqui um
  nome é um parâmetro compartilhado. A família entra com o parâmetro desvinculado da definição (236 casos) ou renomeado
  `Nome 2` e desvinculado (281 casos), sempre com seus valores e com nota.
- **Fórmulas** que citam um parâmetro que a família perdeu: ficam de fora (32 casos) e o parâmetro guarda o valor de cada
  tipo.

O que isso custa, e continua limite do núcleo: uma família guarda **64 parâmetros**, e 257 das 550 têm mais (as da Amanco
têm até 90); os que passam ficam de fora, listados na nota de cada família.

Ainda não rodou: o botão em si (passo 4), e o `ELE-TIPO1.rvt` com todas as suas famílias.
