<?php

$value = htmlspecialchars($_GET['x']);
system('ls -l ' . $value);
