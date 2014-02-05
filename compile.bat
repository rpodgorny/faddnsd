setlocal

rd /s /q build
rd /s /q dist

del *.pyc

;rem python setup.py py2exe
python setup.py bdist

del *.pyc

;rem rd /s /q build
;rem del dist\w9xpopen.exe

;rem copy dist\*.exe .\

;rem rd /s /q dist