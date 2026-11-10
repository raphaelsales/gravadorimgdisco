# Gravador de Imagens

Aplicativo em português que orienta, passo a passo, a gravação de imagens ISO
em CD/DVD e a criação de um desenho a partir de uma foto para CD.

## Recursos

- Verificação prévia da imagem e da compatibilidade do disco com o gravador.
- Fluxo guiado em seis etapas: Escolher, Disco, Verificação, Opções, Gravar e
  Pronto.
- Modo de treinamento com um gravador simulado, sem gravar no disco.
- Criação dos arquivos de uma faixa de áudio a partir de uma foto, para gravação
  posterior no ImgBurn.

## Baixar e usar

Os pacotes de distribuição estão em [`export/`](export/):

- **macOS:** baixe [`Gravador de Imagens - Mac.zip`](export/Mac/Gravador%20de%20Imagens%20-%20Mac.zip), descompacte e abra o aplicativo. Na primeira abertura, talvez seja necessário clicar com o botão direito e selecionar **Abrir**.
- **Windows:** baixe [`Gravador de Imagens - Windows.zip`](export/Windows/Gravador%20de%20Imagens%20-%20Windows.zip), descompacte e execute `Gravador de Imagens.exe`. O Windows pode exibir um aviso porque o programa não tem assinatura digital.

Para instruções detalhadas de uso, consulte [`COMO-USAR.txt`](COMO-USAR.txt).

## Compilar

O projeto usa Rust e Cargo. Com a toolchain do Rust instalada, compile e execute
o aplicativo no macOS ou Windows:

```sh
cargo build --release
cargo run
```

No macOS, o script abaixo cria o aplicativo e o ZIP de distribuição em
`export/Mac/`:

```sh
./package.sh
```

Para também gerar a versão Windows a partir do macOS, instale o alvo
`x86_64-pc-windows-gnu` do Rust e o compilador MinGW (`mingw-w64`) antes de
executar o script. Sem essas ferramentas, a etapa Windows é ignorada.

## Limitações e avisos

- A gravação real usa os recursos do sistema: `drutil`/`hdiutil` no macOS e
  IMAPI2 no Windows.
- O gravador simulado é apenas para treinamento; ele não grava discos.
- A gravação em Windows e o desenho em discos CD-R ELGIN ainda não foram
  testados com discos reais. Faça um teste antes de gravar discos importantes.
- Para criar uma faixa de desenho, reserve aproximadamente 1 GB de espaço
  livre.

## Licença

GPL-3.0-or-later. Consulte [`NOTICE.md`](NOTICE.md) para os avisos de
atribuição de componentes e dados adaptados.
