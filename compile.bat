setlocal

rd /s /q build
rd /s /q dist

del *.pyc

;rem python setup.py py2exe
python setup.py bdist_egg

del *.pyc

rd /s /q build
del dist\w9xpopen.exe

copy dist\*.exe .\

rd /s /q dist