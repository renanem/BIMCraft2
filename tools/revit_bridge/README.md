# Ponte Revit → BIMCraft

Exportação de famílias pelo **API público do Revit**, executada como botão do pyRevit. Requer Revit e pyRevit no computador de origem; o BIMCraft continua sem depender deles. O exportador não salva documentos Revit nem confirma transações: a avaliação temporária dos tipos usa rollback.

Sem o Revit, o BIMCraft já lê de um `.rfa` os tipos e seus parâmetros de tipo (nome, tipo de dado, unidade, valor, GUID do parâmetro compartilhado). O que não está nessa parte do arquivo é esta ponte que traz:

- **todos os parâmetros**: de tipo e de instância, com tipo de dado, grupo, GUID do compartilhado, fórmula e se é de relatório;
- **os valores de cada tipo**, já em milímetros, mm², mm³ e graus;
- **a geometria de cada tipo**, como malha triangulada.

**Não é uma conversão integral de famílias paramétricas ou templates.** A geometria é uma malha por tipo, escolhida pelo parâmetro `Imported Type`: você pode posicionar, rotacionar e trocar tipos, mas alterar uma dimensão não redimensiona a malha. As fórmulas vêm como o Revit as escreve; o BIMCraft mantém as que consegue ler e lista as outras no relatório, deixando o valor de cada tipo no parâmetro. Restrições, planos de referência, cotas, materiais, conectores e símbolos 2D não são reconstruídos; a geometria das famílias aninhadas entra na malha, sem a família em si. O sólido hospedeiro de um molde baseado em face ou hospedado não é exportado: é o único que o Revit não deixa apagar (`DocumentValidation.CanDeleteElement`). Tipos de dado sem equivalente (potência, vazão…) entram como número na unidade da família, que o relatório nomeia.

## Instalação do botão

Na pasta de extensões do seu pyRevit, crie:

```text
BIMCraft.extension/
  BIMCraft.tab/
    Exchange.panel/
      Export.pushbutton/
        script.py
        bridge.py
```

Copie os dois arquivos desta pasta para `Export.pushbutton` e recarregue o pyRevit. Abra sua família `.rfa`, ou um projeto criado a partir do seu `.rte`, e clique em **BIMCraft → Exchange → Export**. Escolha uma pasta; o botão cria `BIMCraft-exchange` nela. Uma exportação existente nunca é sobrescrita.

O botão exporta a família ativa, ou todas as famílias carregadas editáveis do projeto, incluindo seus tipos não colocados. Famílias in-place e não editáveis ficam no relatório. O limite é 1.000 tipos e 100.000 triângulos por família; uma família que exceder é recusada, sem truncar sua geometria silenciosamente.

A pasta criada contém:

- `families/NNNN_nome.rfa.json`: um arquivo por família, no formato `bimcraft-revit-family` versão 1 (descrito no topo de `crates/rvt/src/exchange.rs`);
- `NOME.rte.json`: as configurações do projeto ou template, no formato `bimcraft-revit-template` versão 1 (descrito no topo de `crates/rvt/src/template.rs`): níveis, fases, padrões de linha, estilos de linha, padrões de preenchimento, tipos de texto, tipos de cota, estilos de objeto, parâmetros de projeto (os compartilhados com seu GUID), filtros de vista, modelos de vista, carimbos e folhas, mais os nomes dos arquivos das famílias;
- `report.json`: famílias exportadas, recusadas e por quê, e as limitações.

## Abrir no BIMCraft

- **No aplicativo:** aba **Insert → Revit Family**, escolha o `.rfa.json`. A tela mostra o nome, a categoria, os tipos, cada parâmetro e o relatório do que ficou de fora antes de importar.
- **Pela linha de comando**, num projeto existente:

```powershell
bimcraft-cli run "C:\projetos\existente.bimc" "C:\exportacao\BIMCraft-exchange\families\0000_Tomada.rfa.json" --save "C:\projetos\com-familia.bimc"
```

- **O template inteiro** (níveis, fases, padrões e estilos de linha, padrões de preenchimento, tipos de texto e de cota, estilos de objeto, parâmetros de projeto, filtros, modelos de vista, carimbos, folhas e todas as famílias exportadas): abra o `NOME.rte.json` em **File → Open**, confira o relatório e use **Save As** com a extensão `.bimt`. O template salvo passa a aparecer em **File → New → Project from Template…**. Pela linha de comando:

```powershell
bimcraft-cli run "C:\exportacao\BIMCraft-exchange\Escritorio.rte.json" --save "C:\modelos\Escritorio.bimt"
```

O que um comando do BIMCraft recusar (um padrão com segmento fora do limite, um nível repetido) fica de fora, pelo nome, com o motivo, no relatório. **Categorias.** Para parâmetros de projeto e filtros, toda categoria MEP do Revit tem destino: tubos, tubos flexíveis e espaços reservados de tubo são `pipe`; dutos e dutos flexíveis, `duct`; eletrodutos, eletrocalhas e fios têm a sua; e tudo que uma família faz (conexões, acessórios, peças hidráulicas, sprinklers, equipamentos mecânicos e elétricos, luminárias, dispositivos de dados, telefonia, comunicação, alarme de incêndio, segurança e chamada de enfermagem) é `component`, que é como o BIMCraft trata componentes em parâmetros e filtros. Sistemas (de tubulação, de dutos), isolamento e revestimento, e vistas não têm categoria de parâmetro no BIMCraft: saem da lista, com aviso no relatório. Nos estilos de objeto e nas categorias ocultas de um modelo de vista o mapa é um para um, então uma categoria sem linha própria no BIMCraft (conexão de duto, sprinkler) fica de fora e é contada.

**Filtros e modelos de vista.** Um filtro entra com categorias e regras. Regras unidas por OU, ou uma regra numérica sobre parâmetro interno do Revit (cuja unidade a ponte não sabe converter), deixam o filtro de fora, com o motivo. Um modelo de vista entra para plantas, cortes/elevações e 3D, com escala, nível de detalhe, estilo visual, categorias ocultas, filtros aplicados, fase e o que ele controla; modelos de tabela, folha ou vista de desenho ficam de fora.

**Carimbos e folhas.** Do carimbo vêm as linhas e os textos fixos; os rótulos (que a API do Revit não entrega como rótulos) não vêm. A folha vem com número, nome, tamanho e carimbo.

Ainda **não** são exportados: espessuras de linha (a API do Revit não as entrega), a marca de extremidade dos tipos de cota, os rótulos dos carimbos, as sobreposições gráficas de modelos de vista e filtros, o intervalo de vista e o filtro de fase dos modelos de vista, o que cada folha contém, e o modelo em si.

Cada família carregada responde com o que entrou e, em `skipped`, o que ficou de fora e por quê. Uma família de mesmo nome já existente no projeto é atualizada pelo comando `family`; a tela do aplicativo pede outro nome em vez disso. Portas e janelas mantêm a categoria só quando têm os parâmetros de comprimento `Width` e `Height`; sem eles entram como genéricas, e seus hospedeiros e aberturas não são reconstruídos. Categorias sem equivalente também são genéricas.

## Templates e a interface Sistemas

**File → Open** agora oferece `.rte` quando o aplicativo foi compilado com o leitor Revit. O resultado é um projeto novo sem caminho de salvamento: o template original não é sobrescrito. O leitor usa os mesmos elementos suportados no `.rvt` e exibe uma nota sobre o que não reconstrói. Não importa todas as definições sem uso, templates de vista, filtros, folhas, tabelas, parâmetros de projeto/compartilhados ou preferências de roteamento. Para levar a biblioteca carregada, use a ponte acima separadamente.

A aba **Systems / Sistemas** reúne tubos, peças hidráulicas, conexões, válvulas, equipamentos elétricos, luminárias, eletrodutos, bandejas e fiação. **Device** oferece dispositivos elétricos, dados e telefonia, com seletor de tipos filtrado. São as ferramentas de modelagem existentes, com seus comandos e undo. Não executam dimensionamento.

Ainda faltam para o fluxo completo solicitado: reconstruir formas e relações paramétricas do Revit, transferir todas as configurações de template e acrescentar categorias específicas de comunicação, alarme de incêndio, dispositivos de iluminação, chamada de enfermagem e segurança, conexões de eletroduto/bandeja e criação de trajetos paralelos. As opções sem implementação não aparecem como comandos operacionais.

## Verificação

```powershell
python -m unittest discover -s tools/revit_bridge -p test_bridge.py
```

Para incluir o teste de integração com um CLI compilado, defina `BIMCRAFT_CLI` com o caminho absoluto de `bimcraft-cli.exe` antes de executar os testes.

Os testes usam apenas dados sintéticos. O formato gerado foi exercitado no CLI nativo: dois tipos com geometrias diferentes, parâmetro de instância, fórmula, parâmetro compartilhado, e salvamento/reabertura. A leitura de famílias (`read_family`) foi executada dentro do **Revit 2027** (2027.0, pyRevit 6.4, IronPython 2.7.12 sobre .NET 10) em 2026-11-14, por `pyrevit run`, em duas famílias da biblioteca instalada com o Revit, abertas no lugar e fechadas sem salvar: uma baseada em face, com quatro tipos, fórmulas e formas unidas, e uma com famílias aninhadas. A exportação foi carregada no BIMCraft e conferida: a caixa da malha de cada tipo é igual ao seu parâmetro de comprimento, e as quatro fórmulas são avaliadas. Essa execução corrigiu três defeitos: o nome da família vinha vazio num documento de família aberto direto (`OwnerFamily.Name`), comprimentos de tipo de dado específico saíam como número, e o sólido hospedeiro do molde baseado em face era exportado no lugar das formas unidas.

Ainda **não** foi executado dentro do Revit: o botão em si (o diálogo de pasta e a mensagem final), a exportação completa a partir de um projeto (a leitura das famílias carregadas por `EditFamily` rodou em 21 famílias de um projeto real antes de ser interrompida) e a leitura das configurações do template (fases, padrões e estilos de linha, padrões de preenchimento, tipos de texto e de cota, estilos de objeto, parâmetros de projeto, filtros, modelos de vista, carimbos e folhas). A rodada que cobre isso está em [VERIFY.md](VERIFY.md): `headless.py` roda a ponte no Revit sem diálogos e `check_export.py` confere o resultado no BIMCraft.

Limitação conhecida, vista nessa validação: uma família aninhada de vazio que só corta ao ser carregada num projeto ("Cut with Voids When Loaded") não corta dentro do documento da família, então o hospedeiro sai maciço e o vazio sai como sólido dentro dele. Nenhuma família, template ou outro conteúdo Autodesk é distribuído neste repositório.

Referências do API público: [FamilyManager.CurrentType](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API-MainReference/files/html/3d37cd81-48ba-4011-82bc-dbb7ae14b270.htm), [transações e rollback](https://help.autodesk.com/cloudhelp/2026/ENU/Revit-API/files/Revit_API_Developers_Guide/Basic_Interaction_with_Revit_Elements/Transactions/Revit_API_Revit_API_Developers_Guide_Basic_Interaction_with_Revit_Elements_Transactions_Transaction_Classes_html.html). A ponte usa essas APIs; não adiciona novas descobertas sobre o formato binário fechado.
